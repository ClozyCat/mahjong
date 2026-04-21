import type { FanGuideEntry } from './fanGuide';

interface FanGuideCardProps {
  entry: FanGuideEntry;
  className?: string;
}

export function FanGuideCard({ entry, className }: FanGuideCardProps) {
  const resolvedClassName = ['fan-guide__card', className].filter(Boolean).join(' ');

  return (
    <article className={resolvedClassName}>
      <div className="fan-guide__card-head">
        <div className="fan-guide__card-title">
          <strong>{entry.label}</strong>
        </div>
        <div className="fan-guide__fan-pill" aria-label={`${entry.fanValue}番`}>
          <span className="fan-guide__pill-value">{entry.fanValue}</span>
          <span className="fan-guide__pill-unit">番</span>
        </div>
      </div>
      <div className="fan-guide__card-body">
        <p className="fan-guide__card-copy">{entry.intro}</p>
        <div className="fan-guide__example">
          <span className="fan-guide__example-label">事例：</span>
          {entry.example}
        </div>
      </div>
    </article>
  );
}
