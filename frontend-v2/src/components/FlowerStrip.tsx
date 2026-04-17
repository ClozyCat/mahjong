import { TileFace } from "./TileFace";

export function FlowerStrip({ flowers }: { flowers: string[] }) {
  if (flowers.length === 0) return null;
  return (
    <div className="flower-strip">
      {flowers.map((f, i) => (
        <TileFace key={`${f}-${i}`} tileKey={f} size="sm" />
      ))}
    </div>
  );
}
