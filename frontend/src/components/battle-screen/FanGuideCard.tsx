import { memo } from 'react';
import type { CSSProperties } from 'react';
import type { FanGuideEntry } from './fanGuide';

interface FanGuideCardProps {
  entry: FanGuideEntry;
  className?: string;
  isPinned?: boolean;
  onPin?: () => void;
}

/**
 * Interpolates between five colors with custom value ranges:
 * 1-4: Green -> Blue
 * 6-12: Blue -> Purple
 * 16-32: Purple -> Gold
 * 48-88: Gold -> Red
 * @param value Fan value
 */
export function getFanColor(value: number): string {
  const stops = [
    { v: 1, c: { r: 34, g: 84, b: 61 } },    // Green (1 Fan)
    { v: 4, c: { r: 28, g: 60, b: 118 } },   // Blue (4 Fan)
    { v: 6, c: { r: 28, g: 60, b: 118 } },   // Blue (6 Fan)
    { v: 12, c: { r: 91, g: 46, b: 122 } },  // Purple (12 Fan)
    { v: 16, c: { r: 91, g: 46, b: 122 } },  // Purple (16 Fan)
    { v: 32, c: { r: 166, g: 124, b: 0 } },  // Gold (32 Fan)
    { v: 48, c: { r: 166, g: 124, b: 0 } },  // Gold (48 Fan)
    { v: 88, c: { r: 158, g: 26, b: 47 } },  // Red (88 Fan)
  ];

  if (value <= stops[0].v) return `rgb(${stops[0].c.r}, ${stops[0].c.g}, ${stops[0].c.b})`;
  if (value >= stops[stops.length - 1].v) return `rgb(${stops[stops.length - 1].c.r}, ${stops[stops.length - 1].c.g}, ${stops[stops.length - 1].c.b})`;

  for (let i = 0; i < stops.length - 1; i++) {
    const s = stops[i];
    const e = stops[i + 1];

    if (value >= s.v && value <= e.v) {
      if (s.v === e.v) return `rgb(${s.c.r}, ${s.c.g}, ${s.c.b})`;
      const t = (value - s.v) / (e.v - s.v);
      const r = Math.round(s.c.r + (e.c.r - s.c.r) * t);
      const g = Math.round(s.c.g + (e.c.g - s.c.g) * t);
      const b = Math.round(s.c.b + (e.c.b - s.c.b) * t);
      return `rgb(${r}, ${g}, ${b})`;
    }
  }

  return `rgb(${stops[0].c.r}, ${stops[0].c.g}, ${stops[0].c.b})`;
}

export const FanGuideCard = memo(function FanGuideCard({ entry, className, isPinned, onPin }: FanGuideCardProps) {
  const resolvedClassName = ['fan-guide__card', className].filter(Boolean).join(' ');
  const fanBg = getFanColor(entry.fanValue);

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
        {onPin && (
          <button
            type="button"
            className={`fan-guide__pin-btn ${isPinned ? 'fan-guide__pin-btn--active' : ''}`}
            onClick={(e) => {
              e.stopPropagation();
              onPin();
            }}
            title={isPinned ? '取消固定' : '固定到界面'}
            aria-label={isPinned ? '取消固定' : '固定在此番种'}
          >
            <PinIcon />
          </button>
        )}
      </div>
      <div className="fan-guide__card-body">
        <p className="fan-guide__card-copy">{entry.intro}</p>
        <div className="fan-guide__example">
          <span className="fan-guide__example-label">例：</span>
          {entry.example}
        </div>
      </div>
    </article>
  );
});

function PinIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
      <line x1="12" y1="17" x2="12" y2="22"></line>
      <path d="M5 17h14v-1.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V6a3 3 0 0 0-3-3 3 3 0 0 0-3 3v4.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24Z"></path>
    </svg>
  );
}
