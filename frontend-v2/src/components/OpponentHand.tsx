import { TileBack } from "./TileFace";

interface Props {
  count: number;
  orientation: "horizontal" | "vertical";
  size?: "lg" | "md" | "sm";
}

export function OpponentHand({ count, orientation, size = "md" }: Props) {
  const cls = `opponent-hand ${orientation === "vertical" ? "vertical" : ""}`;
  return (
    <div className={cls}>
      {Array.from({ length: count }).map((_, i) => (
        <TileBack
          key={i}
          size={size}
          orientation={orientation === "vertical" ? "v" : "h"}
        />
      ))}
    </div>
  );
}
