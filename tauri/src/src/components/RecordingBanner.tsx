import { Loader2 } from "lucide-react";

function formatTime(secs: number): string {
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const s = secs % 60;
  const pad = (n: number) => n.toString().padStart(2, "0");
  return h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${pad(m)}:${pad(s)}`;
}

type Props = {
  recording: boolean;
  processing: boolean;
  processingStage: string;
  elapsedSecs: number;
};

export function RecordingBanner({
  recording,
  processing,
  processingStage,
  elapsedSecs,
}: Props) {
  return (
    <div className="flex flex-col items-center justify-center py-lg px-md border-b border-border-subtle bg-elevated">
      {recording ? (
        <>
          <div className="flex items-center gap-sm mb-sm">
            <div className="w-2.5 h-2.5 rounded-full bg-recording animate-recording-pulse" />
            <span className="text-xs font-medium uppercase tracking-wider text-recording">
              Recording
            </span>
          </div>
          <span className="text-3xl font-light tabular-nums tracking-tight text-text">
            {formatTime(elapsedSecs)}
          </span>
          {/* Waveform visualization */}
          <div className="flex items-center gap-[3px] mt-md h-6">
            {Array.from({ length: 24 }).map((_, i) => (
              <div
                key={i}
                className="w-[2px] bg-recording/60 rounded-full"
                style={{
                  height: `${4 + Math.random() * 16}px`,
                  animation: `waveform ${0.8 + Math.random() * 0.6}s ease-in-out ${i * 0.05}s infinite`,
                }}
              />
            ))}
          </div>
        </>
      ) : processing ? (
        <>
          <Loader2 size={20} className="text-accent-blue animate-spin mb-sm" />
          <span className="text-xs text-text-secondary capitalize">
            {processingStage || "Processing..."}
          </span>
        </>
      ) : null}
    </div>
  );
}
