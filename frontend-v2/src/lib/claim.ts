// 听牌候选生成:吃/碰/杠/胡 的本地 tile id 组合
import { normalizeTileKey } from "./tileUtils";
import type { TileRef } from "../types/protocol";

export interface ClaimCandidate {
  action: "chow" | "pung" | "kong";
  tileIds: string[];
  previewKeys: string[];
}

function sameKey(a: string, b: string) {
  return normalizeTileKey(a) === normalizeTileKey(b);
}

export function buildChowCandidates(
  hand: TileRef[],
  targetKey: string,
): ClaimCandidate[] {
  const key = normalizeTileKey(targetKey);
  const suit = key[0];
  if (!["w", "t", "b"].includes(suit)) return [];
  const rank = Number(key[1]);
  if (!rank) return [];
  const byKey = new Map<string, TileRef[]>();
  for (const t of hand) {
    const nk = normalizeTileKey(t.tile_key);
    if (nk[0] !== suit) continue;
    const arr = byKey.get(nk) ?? [];
    arr.push(t);
    byKey.set(nk, arr);
  }
  const patterns: [number, number][] = [
    [rank - 2, rank - 1],
    [rank - 1, rank + 1],
    [rank + 1, rank + 2],
  ];
  const out: ClaimCandidate[] = [];
  for (const [a, b] of patterns) {
    if (a < 1 || a > 9 || b < 1 || b > 9) continue;
    const ka = `${suit}${a}`;
    const kb = `${suit}${b}`;
    const ta = byKey.get(ka)?.[0];
    const tb = byKey.get(kb)?.[0];
    if (!ta || !tb) continue;
    out.push({
      action: "chow",
      tileIds: [ta.tile_id, tb.tile_id],
      previewKeys: [ka, key, kb].sort(),
    });
  }
  return out;
}

export function buildPungCandidates(
  hand: TileRef[],
  targetKey: string,
): ClaimCandidate[] {
  const matches = hand.filter((t) => sameKey(t.tile_key, targetKey));
  if (matches.length < 2) return [];
  return [
    {
      action: "pung",
      tileIds: [matches[0].tile_id, matches[1].tile_id],
      previewKeys: [targetKey, targetKey, targetKey],
    },
  ];
}

export function buildClaimKongCandidates(
  hand: TileRef[],
  targetKey: string,
): ClaimCandidate[] {
  const matches = hand.filter((t) => sameKey(t.tile_key, targetKey));
  if (matches.length < 3) return [];
  return [
    {
      action: "kong",
      tileIds: matches.slice(0, 3).map((t) => t.tile_id),
      previewKeys: [targetKey, targetKey, targetKey, targetKey],
    },
  ];
}

// 本家回合 kong 候选:暗杠(4张)或补杠(1张已在副露刻子)
export function buildSelfKongCandidates(
  hand: TileRef[],
  melds: string[][],
): ClaimCandidate[] {
  const out: ClaimCandidate[] = [];
  const byKey = new Map<string, TileRef[]>();
  for (const t of hand) {
    const nk = normalizeTileKey(t.tile_key);
    const arr = byKey.get(nk) ?? [];
    arr.push(t);
    byKey.set(nk, arr);
  }
  for (const [nk, ts] of byKey.entries()) {
    if (ts.length >= 4) {
      out.push({
        action: "kong",
        tileIds: ts.slice(0, 4).map((t) => t.tile_id),
        previewKeys: [nk, nk, nk, nk],
      });
    }
  }
  // 补杠:副露存在刻子 + 手里有第 4 张
  for (const meld of melds) {
    if (meld.length !== 3) continue;
    const nk = normalizeTileKey(meld[0]);
    if (!meld.every((k) => normalizeTileKey(k) === nk)) continue;
    const match = byKey.get(nk);
    if (match && match.length >= 1) {
      out.push({
        action: "kong",
        tileIds: [match[0].tile_id],
        previewKeys: [nk, nk, nk, nk],
      });
    }
  }
  return out;
}
