import { Users, CheckCircle2 } from "lucide-react";

type Props = {
  title: string;
  duration: string;
  attendees: string[];
  contentType: string;
  lifecycle?: string[];
  onClick: () => void;
};

export function MeetingCard({
  title,
  duration,
  attendees,
  contentType,
  lifecycle,
  onClick,
}: Props) {
  const isComplete = lifecycle && lifecycle.length > 0;
  const isMemo = contentType === "Memo";

  return (
    <button
      onClick={onClick}
      className="w-full text-left px-md py-sm rounded-lg border border-border-subtle hover:border-border hover:bg-hover transition-all group"
    >
      <div className="flex items-start justify-between">
        <div className="min-w-0 flex-1">
          <p className="text-sm font-medium text-text truncate group-hover:text-text">
            {title}
          </p>
          <div className="flex items-center gap-sm mt-xs">
            <span className="text-xs text-text-tertiary">{duration}</span>
            {!isMemo && attendees.length > 0 && (
              <>
                <span className="text-text-tertiary">·</span>
                <span className="flex items-center gap-xs text-xs text-text-tertiary">
                  <Users size={11} />
                  {attendees.length}
                </span>
              </>
            )}
            {isMemo && (
              <span className="text-xs text-text-tertiary px-xs py-[1px] rounded bg-elevated border border-border-subtle">
                memo
              </span>
            )}
          </div>
        </div>
        {isComplete && (
          <CheckCircle2
            size={14}
            className="text-accent-green shrink-0 mt-[3px]"
          />
        )}
      </div>
    </button>
  );
}
