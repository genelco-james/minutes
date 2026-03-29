import { MeetingCard } from "./MeetingCard";

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

type Props = {
  meetings: Meeting[];
  onSelect: (path: string) => void;
};

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
    else {
      label = d.toLocaleDateString("en-US", {
        month: "long",
        year: "numeric",
      });
    }

    if (!groups[label]) groups[label] = [];
    groups[label].push(m);
  }

  return groups;
}

export function MeetingList({ meetings, onSelect }: Props) {
  const groups = groupByDate(meetings);
  const groupKeys = Object.keys(groups);

  if (meetings.length === 0) {
    return (
      <div className="flex-1 flex items-center justify-center">
        <div className="text-center">
          <p className="text-sm text-text-secondary">No meetings yet</p>
          <p className="text-xs text-text-tertiary mt-xs">
            Click Record to capture your first meeting
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="flex-1 overflow-y-auto px-md py-sm">
      {groupKeys.map((label) => (
        <div key={label} className="mb-md">
          <h3 className="text-[11px] font-medium uppercase tracking-wider text-text-tertiary mb-sm px-xs">
            {label}
          </h3>
          <div className="flex flex-col gap-xs">
            {groups[label].map((m, idx) => (
              <div
                key={m.path}
                className="stagger-item"
                style={{ animationDelay: `${idx * 30}ms` }}
              >
                <MeetingCard
                  title={m.title}
                  duration={m.duration}
                  attendees={m.attendees}
                  contentType={m.content_type}
                  lifecycle={m.lifecycle}
                  onClick={() => onSelect(m.path)}
                />
              </div>
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}
