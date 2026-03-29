import { Circle, Square, StickyNote, Keyboard } from "lucide-react";

type Props = {
  recording: boolean;
  onToggleRecording: () => void;
  onNoteClick: () => void;
};

export function ActionBar({ recording, onToggleRecording, onNoteClick }: Props) {
  return (
    <div className="flex items-center justify-between px-md py-sm border-t border-border-subtle bg-elevated shrink-0">
      {/* Record / Stop button */}
      <button
        onClick={onToggleRecording}
        className={`flex items-center gap-sm px-md py-xs rounded-md text-xs font-medium transition-all ${
          recording
            ? "bg-accent-red-bg text-recording hover:bg-accent-red/20"
            : "bg-hover text-text hover:bg-border"
        }`}
      >
        {recording ? (
          <>
            <Square size={12} fill="currentColor" />
            <span>Stop</span>
          </>
        ) : (
          <>
            <Circle size={12} fill="currentColor" className="text-recording" />
            <span>Record</span>
          </>
        )}
      </button>

      <div className="flex items-center gap-xs">
        {/* Quick note */}
        <button
          onClick={onNoteClick}
          className="flex items-center gap-xs px-sm py-xs rounded-md text-xs text-text-secondary hover:bg-hover hover:text-text transition-colors"
          title="Add a note (⌘+Shift+N)"
        >
          <StickyNote size={14} />
          <span>Note</span>
        </button>

        {/* Keyboard shortcuts hint */}
        <button
          className="p-xs rounded-md hover:bg-hover transition-colors"
          title="Keyboard shortcuts"
        >
          <Keyboard size={14} className="text-text-tertiary" />
        </button>
      </div>
    </div>
  );
}
