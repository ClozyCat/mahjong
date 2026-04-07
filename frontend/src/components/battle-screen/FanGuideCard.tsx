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
        <span className="fan-guide__fan-pill">{entry.fanValue} 番</span>
      </div>
      <p className="fan-guide__card-copy">{entry.intro}</p>
      <p className="fan-guide__example">{entry.example}</p>
    </article>
  );
}
