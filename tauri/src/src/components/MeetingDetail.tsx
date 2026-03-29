import { useEffect, useState } from "react";
import {
  ArrowLeft,
  Share2,
  FileText,
  ExternalLink,
  ChevronDown,
  ChevronRight,
} from "lucide-react";
import { getMeetingDetail } from "../lib/tauri";

type Props = {
  path: string;
  onBack: () => void;
};

export function MeetingDetail({ path, onBack }: Props) {
  const [detail, setDetail] = useState<{
    frontmatter: Record<string, unknown>;
    body: string;
  } | null>(null);
  const [transcriptOpen, setTranscriptOpen] = useState(false);

  useEffect(() => {
    getMeetingDetail(path)
      .then(setDetail)
      .catch(() => {});
  }, [path]);

  if (!detail) {
    return (
      <div className="flex-1 flex items-center justify-center">
        <span className="text-xs text-text-tertiary">Loading...</span>
      </div>
    );
  }

  const { frontmatter, body } = detail;
  const title = (frontmatter.title as string) || path.split("/").pop() || "";
  const date = frontmatter.date as string;
  const duration = frontmatter.duration as string;
  const attendees = (frontmatter.attendees || frontmatter.people || []) as string[];
  const decisions = (frontmatter.decisions || []) as Array<{
    text: string;
    context?: string;
  }>;
  const actionItems = (frontmatter.action_items || []) as Array<{
    text: string;
    owner?: string;
    due?: string;
  }>;

  // Split body into summary and transcript
  const sections = body.split(/^## /m).filter(Boolean);
  const summarySection = sections.find((s) =>
    s.toLowerCase().startsWith("summary")
  );
  const transcriptSection = sections.find((s) =>
    s.toLowerCase().startsWith("transcript")
  );

  return (
    <div className="flex flex-col h-screen bg-bg view-enter">
      {/* Header */}
      <header className="flex items-center gap-sm px-md py-sm border-b border-border-subtle shrink-0">
        <button
          onClick={onBack}
          className="p-xs rounded-md hover:bg-hover transition-colors"
        >
          <ArrowLeft size={16} className="text-text-secondary" />
        </button>
        <h1 className="text-sm font-medium text-text truncate flex-1">
          {title}
        </h1>
      </header>

      {/* Content */}
      <div className="flex-1 overflow-y-auto px-md py-md">
        {/* Metadata */}
        <div className="flex items-center gap-sm text-xs text-text-tertiary mb-md">
          {date && (
            <span>
              {new Date(date).toLocaleDateString("en-US", {
                month: "short",
                day: "numeric",
              })}
            </span>
          )}
          {duration && (
            <>
              <span>·</span>
              <span>{duration}</span>
            </>
          )}
          {attendees.length > 0 && (
            <>
              <span>·</span>
              <span>{attendees.length} people</span>
            </>
          )}
        </div>

        {/* Attendees */}
        {attendees.length > 0 && (
          <div className="flex flex-wrap gap-xs mb-md">
            {attendees.map((a, i) => (
              <span
                key={i}
                className="text-xs px-sm py-[2px] rounded-full bg-elevated border border-border-subtle text-text-secondary"
              >
                {a}
              </span>
            ))}
          </div>
        )}

        {/* Summary */}
        {summarySection && (
          <div className="mb-md">
            <h2 className="text-[11px] font-medium uppercase tracking-wider text-text-tertiary mb-sm">
              Summary
            </h2>
            <p className="text-sm text-text-secondary leading-relaxed selectable">
              {summarySection.replace(/^summary\s*/i, "").trim()}
            </p>
          </div>
        )}

        {/* Decisions */}
        {decisions.length > 0 && (
          <div className="mb-md">
            <h2 className="text-[11px] font-medium uppercase tracking-wider text-text-tertiary mb-sm">
              Decisions
            </h2>
            <ul className="space-y-xs">
              {decisions.map((d, i) => (
                <li
                  key={i}
                  className="text-sm text-text flex items-start gap-sm selectable"
                >
                  <span className="text-accent-blue mt-[2px]">·</span>
                  <span>{d.text || String(d)}</span>
                </li>
              ))}
            </ul>
          </div>
        )}

        {/* Action Items */}
        {actionItems.length > 0 && (
          <div className="mb-md">
            <h2 className="text-[11px] font-medium uppercase tracking-wider text-text-tertiary mb-sm">
              Action Items
            </h2>
            <ul className="space-y-xs">
              {actionItems.map((a, i) => (
                <li key={i} className="flex items-start gap-sm text-sm">
                  <input
                    type="checkbox"
                    className="mt-[3px] rounded border-border accent-accent-blue"
                  />
                  <div className="selectable">
                    {a.owner && (
                      <span className="font-medium text-text">
                        {a.owner}:{" "}
                      </span>
                    )}
                    <span className="text-text-secondary">
                      {a.text || String(a)}
                    </span>
                  </div>
                </li>
              ))}
            </ul>
          </div>
        )}

        {/* Transcript (collapsible) */}
        {transcriptSection && (
          <div className="mb-md">
            <button
              onClick={() => setTranscriptOpen(!transcriptOpen)}
              className="flex items-center gap-xs text-[11px] font-medium uppercase tracking-wider text-text-tertiary hover:text-text-secondary transition-colors mb-sm"
            >
              {transcriptOpen ? (
                <ChevronDown size={12} />
              ) : (
                <ChevronRight size={12} />
              )}
              Transcript
            </button>
            {transcriptOpen && (
              <div className="text-xs text-text-secondary leading-relaxed whitespace-pre-wrap font-mono selectable bg-elevated rounded-lg p-md border border-border-subtle max-h-80 overflow-y-auto">
                {transcriptSection.replace(/^transcript\s*/i, "").trim()}
              </div>
            )}
          </div>
        )}

        {/* Raw body fallback (when no structured sections) */}
        {!summarySection && !transcriptSection && body && (
          <div className="text-sm text-text-secondary leading-relaxed whitespace-pre-wrap selectable">
            {body}
          </div>
        )}
      </div>

      {/* Action bar */}
      <div className="flex items-center justify-between px-md py-sm border-t border-border-subtle bg-elevated shrink-0">
        <button className="flex items-center gap-xs px-sm py-xs rounded-md text-xs text-text-secondary hover:bg-hover hover:text-text transition-colors">
          <Share2 size={14} />
          <span>Share</span>
        </button>
        <button className="flex items-center gap-xs px-sm py-xs rounded-md text-xs text-text-secondary hover:bg-hover hover:text-text transition-colors">
          <FileText size={14} />
          <span>Export</span>
        </button>
        <button className="flex items-center gap-xs px-sm py-xs rounded-md text-xs text-text-secondary hover:bg-hover hover:text-text transition-colors">
          <ExternalLink size={14} />
          <span>Open</span>
        </button>
      </div>
    </div>
  );
}
