use crate::types::{Frontmatter, ParsedMeeting};
use std::path::Path;

/// Split markdown content into frontmatter string and body string.
/// Returns `("", content)` if no frontmatter is found.
pub fn split_frontmatter(content: &str) -> (&str, &str) {
    if !content.starts_with("---") {
        return ("", content);
    }

    if let Some(end) = content[3..].find("\n---") {
        let fm_end = end + 3;
        let body_start = fm_end + 4;
        let body_start = content[body_start..]
            .find('\n')
            .map(|i| body_start + i + 1)
            .unwrap_or(body_start);
        (&content[3..fm_end], &content[body_start..])
    } else {
        ("", content)
    }
}

/// Parse a meeting markdown file into its frontmatter and body.
pub fn parse_meeting(path: &Path) -> Result<ParsedMeeting, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {}", path.display(), e))?;

    let (fm_str, body) = split_frontmatter(&content);
    if fm_str.is_empty() {
        return Err(format!("no frontmatter found in {}", path.display()));
    }

    let frontmatter: Frontmatter = serde_yaml::from_str(fm_str)
        .map_err(|e| format!("failed to parse frontmatter in {}: {}", path.display(), e))?;

    Ok(ParsedMeeting {
        frontmatter,
        body: body.to_string(),
        path: path.to_path_buf(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn split_frontmatter_works() {
        let content = "---\ntitle: Test\ndate: 2026-03-17\n---\n\nBody text here.";
        let (fm, body) = split_frontmatter(content);
        assert!(fm.contains("title: Test"));
        assert!(body.contains("Body text here"));
    }

    #[test]
    fn split_frontmatter_no_frontmatter() {
        let content = "Just plain text.";
        let (fm, body) = split_frontmatter(content);
        assert!(fm.is_empty());
        assert_eq!(body, content);
    }

    #[test]
    fn parse_meeting_reads_valid_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.md");
        std::fs::write(
            &path,
            "---\ntitle: Test Meeting\ntype: meeting\ndate: 2026-03-17T12:00:00-07:00\nduration: 5m\nstatus: complete\ntags: []\nattendees: []\npeople: []\naction_items: []\ndecisions: []\nintents: []\n---\n\n## Transcript\n\nHello world.\n",
        )
        .unwrap();

        let meeting = parse_meeting(&path).unwrap();
        assert_eq!(meeting.frontmatter.title, "Test Meeting");
        assert!(meeting.body.contains("Hello world"));
    }
}
