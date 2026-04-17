import { TileFace } from "./TileFace";
import type { SeatPosition } from "../lib/tileUtils";

interface Props {
  discards: string[];
  latestKey?: string | null;
  position: SeatPosition;
}

const ROTATIONS: Record<SeatPosition, number> = {
  bottom: 0,
  top: 180,
  left: 90,
  right: 270,
};

export function River({ discards, latestKey, position }: Props) {
  const rot = ROTATIONS[position];
  const cls = `river river-${position}`;
  return (
    <div className={cls}>
      {discards.map((tileKey, idx) => {
        const isLatest =
          !!latestKey && idx === discards.length - 1 && tileKey === latestKey;
        return (
          <div className="river-slot" key={`${tileKey}-${idx}`}>
            <div
              className="river-rot"
              style={{ transform: `rotate(${rot}deg)` }}
            >
              <TileFace tileKey={tileKey} size="md" latest={isLatest} />
            </div>
          </div>
        );
      })}
    </div>
  );
}
