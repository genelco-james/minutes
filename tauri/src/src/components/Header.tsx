import { Search, Settings } from "lucide-react";

type Props = {
  onSettingsClick: () => void;
};

export function Header({ onSettingsClick }: Props) {
  return (
    <header className="flex items-center justify-between px-md py-sm border-b border-border-subtle shrink-0">
      <div className="flex items-center gap-sm">
        <div className="w-2 h-2 rounded-full bg-accent-green" />
        <span className="text-xs font-medium tracking-wide uppercase text-text-secondary">
          Minutes
        </span>
      </div>
      <div className="flex items-center gap-xs">
        <button className="p-xs rounded-md hover:bg-hover transition-colors">
          <Search size={16} className="text-text-tertiary" />
        </button>
        <button
          onClick={onSettingsClick}
          className="p-xs rounded-md hover:bg-hover transition-colors"
        >
          <Settings size={16} className="text-text-tertiary" />
        </button>
      </div>
    </header>
  );
}
