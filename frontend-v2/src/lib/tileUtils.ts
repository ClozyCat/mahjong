// Tile key 规范化与展示

export type TileSuit =
  | "wan"
  | "tiao"
  | "tong"
  | "wind"
  | "dragon"
  | "flower"
  | "season"
  | "unknown";

export interface TileDescriptor {
  key: string;
  suit: TileSuit;
  rank: number;
  label: string;
  secondary?: string;
  accent: "default" | "red" | "jade" | "gold";
}

const WIND_MAP: Record<string, string> = {
  east: "東",
  south: "南",
  west: "西",
  north: "北",
};

const DRAGON_MAP: Record<string, string> = {
  red: "中",
  green: "發",
  white: "白",
};

const RANK_CN = ["", "一", "二", "三", "四", "五", "六", "七", "八", "九"];

export function normalizeTileKey(raw: string): string {
  if (!raw) return raw;
  const s = raw.toLowerCase();
  // 兼容别名:m->w, p->b, c->t, d1..d7
  const aliases: Record<string, string> = {
    d1: "east",
    d2: "south",
    d3: "west",
    d4: "north",
    d5: "red",
    d6: "green",
    d7: "white",
  };
  if (aliases[s]) return aliases[s];
  const first = s[0];
  const rest = s.slice(1);
  if (first === "m") return `w${rest}`;
  if (first === "p") return `b${rest}`;
  if (first === "c") return `t${rest}`;
  return s;
}

export function describeTile(rawKey: string): TileDescriptor {
  const key = normalizeTileKey(rawKey);
  // w1..w9 万 (朱砂红文字)
  if (/^w[1-9]$/.test(key)) {
    const r = Number(key[1]);
    return {
      key,
      suit: "wan",
      rank: r,
      label: RANK_CN[r],
      secondary: "萬",
      accent: "red",
    };
  }
  // t1..t9 条 (黛绿)
  if (/^t[1-9]$/.test(key)) {
    const r = Number(key[1]);
    return {
      key,
      suit: "tiao",
      rank: r,
      label: String(r),
      accent: "jade",
    };
  }
  // b1..b9 筒 (几何同心圆)
  if (/^b[1-9]$/.test(key)) {
    const r = Number(key[1]);
    return {
      key,
      suit: "tong",
      rank: r,
      label: String(r),
      accent: r === 1 ? "red" : "default",
    };
  }
  if (WIND_MAP[key]) {
    return {
      key,
      suit: "wind",
      rank: 0,
      label: WIND_MAP[key],
      accent: "default",
    };
  }
  if (DRAGON_MAP[key]) {
    const accent: TileDescriptor["accent"] =
      key === "red" ? "red" : key === "green" ? "jade" : "gold";
    return {
      key,
      suit: "dragon",
      rank: 0,
      label: DRAGON_MAP[key],
      accent,
    };
  }
  if (/^f[1-8]$/.test(key)) {
    const r = Number(key[1]);
    return {
      key,
      suit: "flower",
      rank: r,
      label: String(r),
      secondary: "花",
      accent: "gold",
    };
  }
  if (/^s[1-8]$/.test(key)) {
    const r = Number(key[1]);
    return {
      key,
      suit: "season",
      rank: r,
      label: String(r),
      secondary: "季",
      accent: "gold",
    };
  }
  return { key, suit: "unknown", rank: 0, label: key, accent: "default" };
}

export function isFlowerKey(rawKey: string): boolean {
  const d = describeTile(rawKey);
  return d.suit === "flower" || d.suit === "season";
}

// 相对方位映射
export type SeatPosition = "bottom" | "right" | "top" | "left";

export function seatPosition(
  seatIndex: number,
  localSeat: number,
): SeatPosition {
  const rel = (seatIndex - localSeat + 4) % 4;
  switch (rel) {
    case 0:
      return "bottom";
    case 1:
      return "right";
    case 2:
      return "top";
    case 3:
      return "left";
    default:
      return "bottom";
  }
}

export function groupSameKey(ids: { tile_id: string; tile_key: string }[]) {
  const map = new Map<string, string[]>();
  for (const t of ids) {
    const key = normalizeTileKey(t.tile_key);
    const arr = map.get(key) ?? [];
    arr.push(t.tile_id);
    map.set(key, arr);
  }
  return map;
}

export function countByKey(keys: string[]): Map<string, number> {
  const m = new Map<string, number>();
  for (const k of keys) {
    const nk = normalizeTileKey(k);
    m.set(nk, (m.get(nk) ?? 0) + 1);
  }
  return m;
}
