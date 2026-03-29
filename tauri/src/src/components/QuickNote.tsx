import { useState, useRef, useEffect } from "react";
import { X } from "lucide-react";
import { addNote } from "../lib/tauri";

type Props = {
  onClose: () => void;
};

export function QuickNote({ onClose }: Props) {
  const [text, setText] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    textareaRef.current?.focus();
  }, []);

  const handleSubmit = async () => {
    if (!text.trim() || submitting) return;
    setSubmitting(true);
    try {
      await addNote(text.trim());
      onClose();
    } catch {
      setSubmitting(false);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSubmit();
    }
    if (e.key === "Escape") {
      onClose();
    }
  };

  return (
    <div className="fixed inset-0 bg-bg/80 backdrop-blur-sm flex items-center justify-center z-50 backdrop-enter">
      <div className="w-[340px] bg-elevated rounded-lg border border-border shadow-lg">
        {/* Header */}
        <div className="flex items-center justify-between px-sm py-xs border-b border-border-subtle">
          <span className="text-xs font-medium text-text-secondary">
            Quick Note
          </span>
          <button
            onClick={onClose}
            className="p-xs rounded-md hover:bg-hover transition-colors"
          >
            <X size={14} className="text-text-tertiary" />
          </button>
        </div>

        {/* Text area */}
        <textarea
          ref={textareaRef}
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="Add a note to the current recording..."
          className="w-full h-28 px-sm py-sm bg-transparent text-sm text-text placeholder:text-text-tertiary resize-none outline-none selectable"
        />

        {/* Footer */}
        <div className="flex items-center justify-between px-sm py-xs border-t border-border-subtle">
          <span className="text-[10px] text-text-tertiary">
            Enter to save · Esc to cancel
          </span>
          <button
            onClick={handleSubmit}
            disabled={!text.trim() || submitting}
            className="px-sm py-[3px] rounded-md text-xs font-medium bg-accent-blue text-white hover:bg-accent-blue/80 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
          >
            Save
          </button>
        </div>
      </div>
    </div>
  );
}
