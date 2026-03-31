//! Query Outlook for Mac (Classic) via AppleScript for meeting invitees.
//!
//! When a Teams meeting is detected, we look up the corresponding calendar
//! event in Outlook and extract the invitee list (name, email, role).
//! This is macOS-only and requires Automation permission for Outlook.

use std::collections::HashSet;

/// An invitee extracted from an Outlook calendar event.
#[derive(Debug, Clone)]
pub struct OutlookInvitee {
    pub name: String,
    pub email: String,
    /// Raw role string from AppleScript, e.g. "required attendee", "optional attendee"
    pub role: String,
}

/// Query Outlook for calendar events near the current time and return invitees
/// from the best-matching event. Returns an empty Vec on any failure.
pub fn get_outlook_invitees(meeting_title: Option<&str>) -> Vec<OutlookInvitee> {
    let log_file = || -> Option<std::fs::File> {
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dirs::home_dir()?.join(".minutes/logs/outlook-debug.log"))
            .ok()
    };
    let mut lf = log_file();
    macro_rules! olog {
        ($($arg:tt)*) => {{
            let msg = format!($($arg)*);
            eprintln!("{}", msg);
            if let Some(ref mut f) = lf {
                use std::io::Write;
                writeln!(f, "[{}] {}", chrono::Local::now().format("%H:%M:%S"), msg).ok();
                f.flush().ok();
            }
        }};
    }

    olog!("[outlook] querying invitees, title={:?}", meeting_title);

    // AppleScript that queries Outlook for calendar events in a -30min/+2h window.
    // The asymmetric window catches meetings that started before detection and
    // meetings scheduled later that haven't started yet.
    // Output format uses ASCII separators:
    //   Events separated by newlines
    //   Within each event: SUBJECT \x1F ATTENDEES_BLOCK
    //   Attendees separated by \x1D (Group Separator)
    //   Within each attendee: NAME \x1E EMAIL \x1E TYPE
    let script = r#"set unitSep to (ASCII character 31)
set recSep to (ASCII character 30)
set grpSep to (ASCII character 29)
set now to current date
set windowStart to now - (30 * 60)
set windowEnd to now + (2 * 60 * 60)
set output to ""

tell application "Microsoft Outlook"
    try
        set matchEvents to every calendar event whose start time ≥ windowStart and start time ≤ windowEnd
    on error
        return ""
    end try
    repeat with evt in matchEvents
        try
            set evtSubject to subject of evt
            set attendeeStr to ""
            try
                set allAttendees to every attendee of evt
                repeat with att in allAttendees
                    if attendeeStr is not "" then
                        set attendeeStr to attendeeStr & grpSep
                    end if
                    set attName to ""
                    set attEmail to ""
                    set attType to "required attendee"
                    try
                        set attName to name of att
                    end try
                    try
                        set attEmail to address of email address of att
                    end try
                    try
                        set attType to type of att as string
                    end try
                    set attendeeStr to attendeeStr & attName & recSep & attEmail & recSep & attType
                end repeat
            end try
            if output is not "" then
                set output to output & linefeed
            end if
            set output to output & evtSubject & unitSep & attendeeStr
        end try
    end repeat
end tell
return output"#;

    let output = match std::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
    {
        Ok(out) => out,
        Err(e) => {
            olog!("[outlook] osascript spawn error: {}", e);
            return Vec::new();
        }
    };

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !output.status.success() {
        olog!("[outlook] osascript failed: exit={} stderr={:?}", output.status, stderr);
        return Vec::new();
    }
    if !stderr.is_empty() {
        olog!("[outlook] osascript stderr: {:?}", stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    olog!("[outlook] raw output: {:?}", stdout);

    if stdout.is_empty() {
        olog!("[outlook] no events found in time window");
        return Vec::new();
    }

    // Parse events from output
    let events = parse_outlook_output(&stdout);
    olog!("[outlook] parsed {} events", events.len());
    for (i, (subj, invitees)) in events.iter().enumerate() {
        olog!("[outlook]   event[{}]: {:?} ({} invitees)", i, subj, invitees.len());
    }

    // Match the best event to the detected meeting title
    let matched = best_matching_event(&events, meeting_title);
    match matched {
        Some((subj, invitees)) => {
            olog!("[outlook] matched event: {:?} ({} invitees)", subj, invitees.len());
            invitees.clone()
        }
        None => {
            olog!("[outlook] no matching event found");
            Vec::new()
        }
    }
}

/// Parse the AppleScript output into a list of (subject, invitees) pairs.
fn parse_outlook_output(output: &str) -> Vec<(String, Vec<OutlookInvitee>)> {
    let unit_sep = '\x1F';
    let rec_sep = '\x1E';
    let grp_sep = '\x1D';

    let mut events = Vec::new();

    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.splitn(2, unit_sep).collect();
        let subject = parts[0].trim().to_string();
        let attendee_block = if parts.len() > 1 { parts[1] } else { "" };

        let mut invitees = Vec::new();

        if !attendee_block.is_empty() {
            for att_str in attendee_block.split(grp_sep) {
                let fields: Vec<&str> = att_str.split(rec_sep).collect();
                if fields.is_empty() {
                    continue;
                }
                let name = fields.first().map(|s| s.trim()).unwrap_or("").to_string();
                let email = fields.get(1).map(|s| s.trim()).unwrap_or("").to_string();
                let role = fields.get(2).map(|s| s.trim()).unwrap_or("required attendee").to_string();

                // Skip empty entries
                if name.is_empty() && email.is_empty() {
                    continue;
                }

                invitees.push(OutlookInvitee { name, email, role });
            }
        }

        events.push((subject, invitees));
    }

    events
}

/// Pick the calendar event that best matches the detected Teams meeting title.
///
/// Strategy:
/// 1. Single event → use it
/// 2. Exact title match (case-insensitive)
/// 3. Substring containment (handles truncated Teams window titles)
/// 4. Word overlap scoring
/// 5. Fallback: first event
fn best_matching_event<'a>(
    events: &'a [(String, Vec<OutlookInvitee>)],
    meeting_title: Option<&str>,
) -> Option<&'a (String, Vec<OutlookInvitee>)> {
    if events.is_empty() {
        return None;
    }
    if events.len() == 1 {
        return Some(&events[0]);
    }

    let title = match meeting_title {
        Some(t) if !t.is_empty() => t,
        _ => return Some(&events[0]),
    };

    let title_lower = title.to_lowercase();
    let title_trimmed = title_lower.trim();

    // Exact match (case-insensitive)
    if let Some(e) = events
        .iter()
        .find(|(s, _)| s.to_lowercase().trim() == title_trimmed)
    {
        return Some(e);
    }

    // Substring containment (either direction)
    if let Some(e) = events.iter().find(|(s, _)| {
        let s_lower = s.to_lowercase();
        s_lower.contains(title_trimmed) || title_trimmed.contains(s_lower.trim())
    }) {
        return Some(e);
    }

    // Word overlap scoring
    let title_words: HashSet<&str> = title_trimmed.split_whitespace().collect();
    if !title_words.is_empty() {
        let best = events.iter().max_by_key(|(s, _)| {
            let s_lower = s.to_lowercase();
            let s_words: HashSet<&str> = s_lower.split_whitespace().collect();
            title_words.intersection(&s_words).count()
        });
        if let Some(e) = best {
            let s_lower = e.0.to_lowercase();
            let s_words: HashSet<&str> = s_lower.split_whitespace().collect();
            if title_words.intersection(&s_words).count() > 0 {
                return Some(e);
            }
        }
    }

    // Fallback: first event
    Some(&events[0])
}
