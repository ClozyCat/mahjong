import { memo } from "react";
import { describeTile } from "../lib/tileUtils";
import type { TileDescriptor } from "../lib/tileUtils";

type Size = "lg" | "md" | "sm" | "xs";

interface TileFaceProps {
  tileKey: string;
  size?: Size;
  className?: string;
  selected?: boolean;
  disabled?: boolean;
  highlight?: boolean;
  latest?: boolean;
  drawn?: boolean;
  onClick?: () => void;
  onDoubleClick?: () => void;
  title?: string;
}

function DotGrid({ rank, accent }: { rank: number; accent: "red" | "gold" | "default" }) {
  // 筒子点数布局
  const redOne = rank === 1 || rank === 5 || rank === 9;
  const style = (n: number) => {
    const isRed =
      accent === "red" ||
      (rank === 5 && n === 4) ||
      (rank === 7 && n < 3);
    return isRed ? "dot red" : accent === "gold" ? "dot gold" : "dot";
  };
  if (rank === 1) {
    return (
      <div className="dot-container">
        <div className="dot-one" />
      </div>
    );
  }
  const cells: number[] = [];
  for (let i = 0; i < rank; i++) cells.push(i);
  let rowsCls = "rows-1";
  let colsCls = "cols-1";
  switch (rank) {
    case 2:
      rowsCls = "rows-2";
      colsCls = "cols-1";
      break;
    case 3:
      rowsCls = "rows-3";
      colsCls = "cols-1";
      break;
    case 4:
      rowsCls = "rows-2";
      colsCls = "cols-2";
      break;
    case 5:
      return (
        <div className="dot-container rows-3 cols-3" style={{ gap: 2 }}>
          <span className={style(0)} />
          <span />
          <span className={style(1)} />
          <span />
          <span className={style(2)} />
          <span />
          <span className={style(3)} />
          <span />
          <span className={style(4)} />
        </div>
      );
    case 6:
      rowsCls = "rows-3";
      colsCls = "cols-2";
      break;
    case 7:
      return (
        <div className="dot-container rows-3 cols-3" style={{ gap: 3 }}>
          <span className={style(0)} />
          <span className={style(1)} />
          <span className={style(2)} />
          <span />
          <span className={style(3)} />
          <span />
          <span className={style(4)} />
          <span className={style(5)} />
          <span className={style(6)} />
        </div>
      );
    case 8:
      rowsCls = "rows-3";
      colsCls = "cols-3";
      break;
    case 9:
      rowsCls = "rows-3";
      colsCls = "cols-3";
      break;
  }
  if (rank === 8) {
    return (
      <div className="dot-container rows-3 cols-3" style={{ gap: 3 }}>
        <span className={style(0)} />
        <span className={style(1)} />
        <span />
        <span className={style(2)} />
        <span className={style(3)} />
        <span className={style(4)} />
        <span />
        <span className={style(5)} />
        <span className={style(6)} />
      </div>
    );
  }
  return (
    <div className={`dot-container ${rowsCls} ${colsCls}`}>
      {cells.map((i) => (
        <span key={i} className={style(i)} />
      ))}
    </div>
  );
  // keep redOne reference (lint)
  void redOne;
}

function BambooGrid({ rank }: { rank: number }) {
  if (rank === 1) {
    return (
      <div className="bamboo-container">
        <div className="bird" />
      </div>
    );
  }
  const bars: number[] = [];
  for (let i = 0; i < rank; i++) bars.push(i);
  const rows = rank <= 3 ? 1 : rank <= 6 ? 2 : 3;
  const perRow = Math.ceil(rank / rows);
  const groups: number[][] = [];
  for (let r = 0; r < rows; r++) {
    const slice = bars.slice(r * perRow, (r + 1) * perRow);
    if (slice.length) groups.push(slice);
  }
  return (
    <div className="bamboo-container" style={{ flexDirection: "column", gap: 4 }}>
      {groups.map((g, idx) => (
        <div
          key={idx}
          style={{
            display: "flex",
            gap: 4,
            justifyContent: "center",
          }}
        >
          {g.map((i) => (
            <span className="bamboo" key={i} />
          ))}
        </div>
      ))}
    </div>
  );
}

function renderFace(desc: TileDescriptor) {
  switch (desc.suit) {
    case "wan":
      return (
        <>
          <span className="char">{desc.label}</span>
          <span className="char-sm">{desc.secondary}</span>
        </>
      );
    case "tong":
      return <DotGrid rank={desc.rank} accent={desc.accent === "red" ? "red" : "default"} />;
    case "tiao":
      return <BambooGrid rank={desc.rank} />;
    case "wind":
      return <span className="char">{desc.label}</span>;
    case "dragon":
      if (desc.key === "white") {
        return <div className="dragon-white-frame" />;
      }
      return <span className="char">{desc.label}</span>;
    case "flower":
    case "season":
      return (
        <>
          <span className="char">{desc.label}</span>
          <span className="char-sm">{desc.secondary}</span>
        </>
      );
    default:
      return <span className="char">{desc.label}</span>;
  }
}

function TileFaceImpl({
  tileKey,
  size = "lg",
  className,
  selected,
  disabled,
  highlight,
  latest,
  drawn,
  onClick,
  onDoubleClick,
  title,
}: TileFaceProps) {
  const desc = describeTile(tileKey);
  const cls = [
    "tile",
    `size-${size}`,
    desc.accent !== "default" ? `accent-${desc.accent}` : "",
    desc.suit === "flower" || desc.suit === "season" ? "flower-tile" : "",
    selected ? "selected" : "",
    disabled ? "disabled" : "",
    highlight ? "highlight-same" : "",
    latest ? "latest" : "",
    drawn ? "drawn" : "",
    onClick && !disabled ? "clickable" : "",
    className ?? "",
  ]
    .filter(Boolean)
    .join(" ");
  return (
    <div
      className={cls}
      onClick={disabled ? undefined : onClick}
      onDoubleClick={disabled ? undefined : onDoubleClick}
      title={title ?? desc.key}
    >
      {renderFace(desc)}
    </div>
  );
}

export const TileFace = memo(TileFaceImpl);

export function TileBack({
  size = "lg",
  orientation = "h",
}: {
  size?: Size;
  orientation?: "h" | "v";
}) {
  const sizeToWH: Record<Size, { w: number; h: number }> = {
    lg: { w: 44, h: 62 },
    md: { w: 30, h: 44 },
    sm: { w: 22, h: 32 },
    xs: { w: 18, h: 24 },
  };
  const base = sizeToWH[size];
  const w = orientation === "v" ? base.h * 0.32 : base.w;
  const h = orientation === "v" ? base.h : base.h * 0.55;
  return (
    <div
      className={`tile back size-${size}`}
      style={{
        width: orientation === "v" ? `${w}px` : undefined,
        height: orientation === "v" ? `${h}px` : undefined,
      }}
    />
  );
}
