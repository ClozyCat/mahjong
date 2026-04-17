import { TileFace } from "./TileFace";

interface Props {
  melds: string[][];
  orientation: "horizontal" | "vertical";
  size?: "lg" | "md" | "sm";
  mirrored?: boolean;
}

export function MeldArea({ melds, orientation, size = "md", mirrored }: Props) {
  if (melds.length === 0) return null;
  const cls = [
    "meld-area",
    orientation === "vertical" ? "vertical" : "",
    mirrored ? "mirrored" : "",
  ]
    .filter(Boolean)
    .join(" ");
  return (
    <div className={cls}>
      {melds.map((meld, mi) => (
        <div
          key={mi}
          className={`meld ${orientation === "vertical" ? "meld-vertical" : ""}`}
        >
          {meld.map((k, ti) => (
            <TileFace
              key={`${mi}-${ti}-${k}`}
              tileKey={k}
              size={size}
              className={ti === 0 ? "rotated" : ""}
            />
          ))}
        </div>
      ))}
    </div>
  );
}
