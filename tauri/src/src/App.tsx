import { useEffect, useState, useCallback } from "react";
import {
  listMeetings,
  getStatus,
  startRecording,
  stopRecording,
  onRecordingStatus,
  onProcessingStatus,
  onLatestArtifact,
} from "./lib/tauri";

type Meeting = {
  path: string;
  title: string;
  date: string;
  duration: string;
  attendees: string[];
  word_count: number;
  content_type: string;
  lifecycle?: string[];
};

function formatTime(secs: number): string {
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const s = secs % 60;
  const pad = (n: number) => n.toString().padStart(2, "0");
  return h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${pad(m)}:${pad(s)}`;
}

function groupByDate(meetings: Meeting[]): Record<string, Meeting[]> {
  const today = new Date();
  today.setHours(0, 0, 0, 0);
  const yesterday = new Date(today);
  yesterday.setDate(yesterday.getDate() - 1);
  const weekAgo = new Date(today);
  weekAgo.setDate(weekAgo.getDate() - 7);
  const groups: Record<string, Meeting[]> = {};
  for (const m of meetings) {
    const d = new Date(m.date);
    d.setHours(0, 0, 0, 0);
    let label: string;
    if (d.getTime() === today.getTime()) label = "Today";
    else if (d.getTime() === yesterday.getTime()) label = "Yesterday";
    else if (d >= weekAgo) label = "This Week";
    else label = d.toLocaleDateString("en-US", { month: "long", year: "numeric" });
    if (!groups[label]) groups[label] = [];
    groups[label].push(m);
  }
  return groups;
}

export default function App() {
  const [meetings, setMeetings] = useState<Meeting[]>([]);
  const [recording, setRecording] = useState(false);
  const [processing, setProcessing] = useState(false);
  const [processingStage, setProcessingStage] = useState("");
  const [elapsedSecs, setElapsedSecs] = useState(0);
  const [selectedMeeting, setSelectedMeeting] = useState<string | null>(null);
  const [detail, setDetail] = useState<Record<string, unknown> | null>(null);
  const [hoveredSection, setHoveredSection] = useState<number | null>(null);
  const [copiedSection, setCopiedSection] = useState<number | null>(null);

  const loadMeetings = useCallback(async () => {
    try {
      const data = await listMeetings();
      setMeetings(data || []);
    } catch (e) {
      console.error("loadMeetings failed:", e);
    }
  }, []);

  useEffect(() => {
    getStatus().then((s) => {
      setRecording(s.recording);
      setProcessing(s.processing);
      if (s.processing_stage) setProcessingStage(s.processing_stage);
    }).catch(() => {});
    loadMeetings();
  }, [loadMeetings]);

  useEffect(() => {
    const unsubs: Array<() => void> = [];
    onRecordingStatus((p) => {
      setRecording(p.recording);
      if (p.elapsed_secs !== undefined) setElapsedSecs(p.elapsed_secs);
      if (!p.recording) { setElapsedSecs(0); loadMeetings(); }
    }).then((fn) => unsubs.push(fn));
    onProcessingStatus((p) => {
      setProcessing(p.stage !== "done");
      setProcessingStage(p.stage);
      if (p.stage === "done") loadMeetings();
    }).then((fn) => unsubs.push(fn));
    onLatestArtifact(() => loadMeetings()).then((fn) => unsubs.push(fn));
    return () => unsubs.forEach((fn) => fn());
  }, [loadMeetings]);

  useEffect(() => {
    if (!recording) return;
    const interval = setInterval(() => setElapsedSecs((s) => s + 1), 1000);
    return () => clearInterval(interval);
  }, [recording]);

  const handleToggleRecording = async () => {
    if (recording) await stopRecording();
    else await startRecording();
  };

  const openDetail = async (path: string) => {
    setSelectedMeeting(path);
    setDetail(null);
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const data = await invoke<Record<string, unknown>>("cmd_get_meeting_detail", { path });
      setDetail(data);
    } catch (e) {
      setDetail({ title: "Error", sections: [{ heading: "Error", content: String(e) }] });
    }
  };

  const groups = groupByDate(meetings);

  // Inline styles — no Tailwind dependency for reliability
  const styles = {
    app: { display: "flex", flexDirection: "column" as const, height: "100vh", background: "#1a1a1c", color: "#ececee", fontFamily: "-apple-system, BlinkMacSystemFont, system-ui, sans-serif", fontSize: "14px" },
    header: { display: "flex", alignItems: "center", justifyContent: "space-between", padding: "8px 16px", borderBottom: "1px solid #333335", flexShrink: 0 },
    headerLeft: { display: "flex", alignItems: "center", gap: "8px" },
    dot: (color: string) => ({ width: 8, height: 8, borderRadius: "50%", background: color }),
    label: { fontSize: "11px", fontWeight: 500, textTransform: "uppercase" as const, letterSpacing: "0.05em", color: "#8e8e93" },
    content: { flex: 1, overflowY: "auto" as const, padding: "8px 16px" },
    groupLabel: { fontSize: "11px", fontWeight: 500, textTransform: "uppercase" as const, letterSpacing: "0.05em", color: "#5c5c60", marginBottom: "8px", marginTop: "16px" },
    card: { width: "100%", textAlign: "left" as const, padding: "8px 12px", borderRadius: "10px", border: "1px solid #2a2a2c", background: "transparent", color: "#ececee", cursor: "pointer", marginBottom: "4px", display: "block" },
    cardTitle: { fontSize: "14px", fontWeight: 500, margin: 0 },
    cardMeta: { fontSize: "12px", color: "#5c5c60", marginTop: "4px" },
    actionBar: { display: "flex", alignItems: "center", justifyContent: "space-between", padding: "8px 16px", borderTop: "1px solid #333335", background: "#242426", flexShrink: 0 },
    recBtn: (isRec: boolean) => ({ display: "flex", alignItems: "center", gap: "8px", padding: "4px 16px", borderRadius: "8px", border: "none", fontSize: "12px", fontWeight: 500, cursor: "pointer", background: isRec ? "rgba(239,68,68,0.12)" : "#2e2e30", color: isRec ? "#ef4444" : "#ececee" }),
    recDot: { width: 10, height: 10, borderRadius: "50%", background: "#ef4444" },
    stopSquare: { width: 10, height: 10, borderRadius: "2px", background: "#ef4444" },
    banner: { display: "flex", flexDirection: "column" as const, alignItems: "center", padding: "24px 16px", borderBottom: "1px solid #333335", background: "#242426" },
    bannerTime: { fontSize: "28px", fontWeight: 300, fontVariantNumeric: "tabular-nums", letterSpacing: "-0.02em" },
    bannerLabel: { fontSize: "11px", fontWeight: 500, textTransform: "uppercase" as const, letterSpacing: "0.05em", color: "#ef4444", marginBottom: "8px", display: "flex", alignItems: "center", gap: "8px" },
    pulsingDot: { width: 10, height: 10, borderRadius: "50%", background: "#ef4444", animation: "recording-pulse 1.5s ease-in-out infinite" },
    backBtn: { background: "none", border: "none", color: "#8e8e93", cursor: "pointer", padding: "4px", fontSize: "14px" },
    detailHeader: { display: "flex", alignItems: "center", gap: "8px", padding: "8px 16px", borderBottom: "1px solid #333335", flexShrink: 0 },
    detailTitle: { fontSize: "14px", fontWeight: 500, flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" as const },
    detailBody: { flex: 1, overflowY: "auto" as const, padding: "16px", fontSize: "13px", lineHeight: 1.6, whiteSpace: "pre-wrap" as const, color: "#8e8e93" },
    empty: { flex: 1, display: "flex", alignItems: "center", justifyContent: "center", flexDirection: "column" as const },
    emptyText: { fontSize: "14px", color: "#8e8e93" },
    emptySubtext: { fontSize: "12px", color: "#5c5c60", marginTop: "4px" },
    noteBtn: { background: "none", border: "none", color: "#8e8e93", cursor: "pointer", fontSize: "12px", padding: "4px 8px" },
  };

  // Detail view
  if (selectedMeeting) {
    if (!detail) {
      return (
        <div style={styles.app}>
          <div style={styles.detailHeader}>
            <button style={styles.backBtn} onClick={() => setSelectedMeeting(null)}>← Back</button>
            <div style={styles.detailTitle}>Loading...</div>
          </div>
          <div style={styles.empty}><div style={styles.emptyText}>Loading...</div></div>
        </div>
      );
    }
    const title = selectedMeeting.split("/").pop()?.replace(/\.md$/, "") || (detail.title as string) || "Meeting";
    const date = (detail.date as string) || "";
    const duration = (detail.duration as string) || "";
    const attendees = Array.isArray(detail.attendees) ? detail.attendees as string[] : [];
    const sections = Array.isArray(detail.sections) ? detail.sections as Array<{ heading: string; content: string }> : [];
    const status = (detail.status as string) || "";
    const contentType = (detail.content_type as string) || "";

    return (
      <div style={styles.app}>
        {/* Header */}
        <div style={styles.detailHeader}>
          <button style={styles.backBtn} onClick={() => { setSelectedMeeting(null); setDetail(null); }}>← Back</button>
          <div style={styles.detailTitle}>{title}</div>
        </div>

        {/* Metadata bar */}
        <div style={{ padding: "8px 16px", fontSize: "12px", color: "#5c5c60", borderBottom: "1px solid #2a2a2c" }}>
          {date && <span>{new Date(date).toLocaleDateString("en-US", { month: "short", day: "numeric" })}</span>}
          {duration && <span> · {duration}</span>}
          {contentType && <span> · {contentType}</span>}
          {status && <span> · {status}</span>}
          {attendees.length > 0 && (
            <div style={{ marginTop: "6px", display: "flex", flexWrap: "wrap", gap: "4px" }}>
              {attendees.map((a, i) => (
                <span key={i} style={{ fontSize: "11px", padding: "2px 8px", borderRadius: "12px", background: "#242426", border: "1px solid #333335", color: "#8e8e93" }}>{a}</span>
              ))}
            </div>
          )}
        </div>

        {/* Sections */}
        <div style={styles.content}>
          {sections.map((section, i) => {
            const isTranscript = section.heading.toLowerCase() === "transcript";
            return (
              <div key={i} style={{ marginBottom: "16px" }}>
                <div style={{ ...styles.groupLabel, marginTop: i === 0 ? "0" : "16px" }}>{section.heading}</div>
                <div
                  style={{ position: "relative", fontSize: "13px", lineHeight: 1.7, whiteSpace: "pre-wrap", color: isTranscript ? "#8e8e93" : "#ececee", fontFamily: isTranscript ? "SF Mono, Menlo, monospace" : "inherit", background: isTranscript ? "#242426" : "transparent", padding: isTranscript ? "12px" : "0", borderRadius: isTranscript ? "10px" : "0", border: isTranscript ? "1px solid #333335" : "none" }}
                  onMouseEnter={() => isTranscript && setHoveredSection(i)}
                  onMouseLeave={() => isTranscript && setHoveredSection(null)}
                >
                  {section.content}
                  {isTranscript && (hoveredSection === i || copiedSection === i) && (
                    <button
                      onClick={() => {
                        navigator.clipboard.writeText(section.content);
                        setCopiedSection(i);
                        setTimeout(() => setCopiedSection(null), 1500);
                      }}
                      style={{ position: "absolute", top: "8px", right: "8px", background: "#333335", border: "none", borderRadius: "6px", padding: "5px", cursor: "pointer", display: "flex", alignItems: "center", justifyContent: "center", color: copiedSection === i ? "#34d399" : "#8e8e93", transition: "color 0.15s" }}
                      title="Copy transcript"
                    >
                      {copiedSection === i ? (
                        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><polyline points="20 6 9 17 4 12" /></svg>
                      ) : (
                        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" ry="2" /><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" /></svg>
                      )}
                    </button>
                  )}
                </div>
              </div>
            );
          })}
          {sections.length === 0 && (
            <div style={{ color: "#5c5c60", fontSize: "13px" }}>No content sections available.</div>
          )}
        </div>

        {/* Action bar */}
        <div style={styles.actionBar}>
          <button style={styles.noteBtn}>Share</button>
          <button style={styles.noteBtn}>Export</button>
          <button style={styles.noteBtn}>Open File</button>
        </div>
      </div>
    );
  }

  // List view
  return (
    <div style={styles.app}>
      <style>{`@keyframes recording-pulse { 0%,100%{opacity:1} 50%{opacity:0.4} }`}</style>
      {/* Header */}
      <div style={styles.header}>
        <div style={styles.headerLeft}>
          <div style={styles.dot(recording ? "#ef4444" : "#34d399")} />
          <span style={styles.label}>{recording ? "Recording" : "Minutes"}</span>
        </div>
      </div>

      {/* Recording banner */}
      {recording && (
        <div style={styles.banner}>
          <div style={styles.bannerLabel}>
            <div style={styles.pulsingDot} />
            <span>Recording</span>
          </div>
          <div style={styles.bannerTime}>{formatTime(elapsedSecs)}</div>
          <button
            onClick={handleToggleRecording}
            style={{ marginTop: "12px", padding: "6px 20px", borderRadius: "6px", border: "1px solid #333335", background: "transparent", color: "#ef4444", fontSize: "12px", fontWeight: 500, cursor: "pointer", fontFamily: "inherit" }}
          >
            Stop Recording
          </button>
        </div>
      )}

      {/* Processing banner */}
      {!recording && processing && (
        <div style={styles.banner}>
          <div style={{ ...styles.label, color: "#60a5fa" }}>{processingStage || "Processing..."}</div>
        </div>
      )}

      {/* Meeting list */}
      {meetings.length === 0 ? (
        <div style={styles.empty}>
          <div style={styles.emptyText}>No meetings yet</div>
          <div style={styles.emptySubtext}>Click Record to capture your first meeting</div>
        </div>
      ) : (
        <div style={styles.content}>
          {Object.entries(groups).map(([label, items]) => (
            <div key={label}>
              <div style={styles.groupLabel}>{label}</div>
              {items.map((m) => (
                <button key={m.path} style={styles.card} onClick={() => openDetail(m.path)}
                  onMouseOver={(e) => { (e.currentTarget as HTMLElement).style.borderColor = "#333335"; (e.currentTarget as HTMLElement).style.background = "#2e2e30"; }}
                  onMouseOut={(e) => { (e.currentTarget as HTMLElement).style.borderColor = "#2a2a2c"; (e.currentTarget as HTMLElement).style.background = "transparent"; }}
                >
                  <div style={styles.cardTitle}>{m.path.split("/").pop()?.replace(/\.md$/, "") || m.title}</div>
                  <div style={styles.cardMeta}>
                    {m.duration}
                    {(m.attendees?.length || 0) > 0 && <span> · {m.attendees.length} people</span>}
                    {m.content_type === "Memo" && <span> · memo</span>}
                  </div>
                </button>
              ))}
            </div>
          ))}
        </div>
      )}

      {/* Action bar */}
      <div style={styles.actionBar}>
        <button style={styles.recBtn(recording)} onClick={handleToggleRecording}>
          {recording ? <div style={styles.stopSquare} /> : <div style={styles.recDot} />}
          <span>{recording ? "Stop" : "Record"}</span>
        </button>
        <button style={styles.noteBtn}>Note</button>
      </div>
    </div>
  );
}
