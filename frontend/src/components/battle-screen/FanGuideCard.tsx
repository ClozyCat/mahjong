import type { FanGuideEntry } from './fanGuide';
import type { CSSProperties } from 'react';

interface FanGuideCardProps {
  entry: FanGuideEntry;
  className?: string;
}

/**
 * Interpolates between three colors (Green -> Yellow -> Red)
 * @param intensity 0 to 1
 */
function getFanColor(intensity: number): string {
  const green = { r: 30, g: 77, b: 43 };    // #1e4d2b
  const yellow = { r: 184, g: 134, b: 11 }; // #b8860b (Dark Goldenrod for white text contrast)
  const red = { r: 166, g: 27, b: 41 };     // #a61b29

  let r, g, b;

  if (intensity <= 0.5) {
    // Interpolate Green to Yellow
    const t = intensity * 2;
    r = Math.round(green.r + (yellow.r - green.r) * t);
    g = Math.round(green.g + (yellow.g - green.g) * t);
    b = Math.round(green.b + (yellow.b - green.b) * t);
  } else {
    // Interpolate Yellow to Red
    const t = (intensity - 0.5) * 2;
    r = Math.round(yellow.r + (red.r - yellow.r) * t);
    g = Math.round(yellow.g + (red.g - yellow.g) * t);
    b = Math.round(yellow.b + (red.b - yellow.b) * t);
  }

  return `rgb(${r}, ${g}, ${b})`;
}

export function FanGuideCard({ entry, className }: FanGuideCardProps) {
  const resolvedClassName = ['fan-guide__card', className].filter(Boolean).join(' ');
  const intensity = Math.min(1, entry.fanValue / 88);
  const fanBg = getFanColor(intensity);

  return (
    <article className={resolvedClassName}>
      <div className="fan-guide__card-head">
        <div className="fan-guide__card-title">
          <strong>{entry.label}</strong>
        </div>
        <div
          className="fan-guide__fan-pill"
          aria-label={`${entry.fanValue}番`}
          style={{ '--fan-bg': fanBg } as CSSProperties}
        >
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
