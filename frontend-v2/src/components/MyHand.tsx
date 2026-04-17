import { TileFace } from "./TileFace";
import type { TileRef } from "../types/protocol";
import { normalizeTileKey } from "../lib/tileUtils";

interface Props {
  tiles: TileRef[];
  drawnTileId: string | null;
  selectedId: string | null;
  restrictedIds: Set<string>;
  optimisticHiddenId: string | null;
  onSelect: (id: string) => void;
  onDoubleDiscard: (id: string) => void;
}

export function MyHand({
  tiles,
  drawnTileId,
  selectedId,
  restrictedIds,
  optimisticHiddenId,
  onSelect,
  onDoubleDiscard,
}: Props) {
  // 排序:花牌不在手中,已由 flowers 承载;这里直接用原顺序但把 drawn 放最后
  const visible = tiles.filter((t) => t.tile_id !== optimisticHiddenId);
  const drawn = drawnTileId
    ? visible.find((t) => t.tile_id === drawnTileId)
    : undefined;
  const others = drawn ? visible.filter((t) => t.tile_id !== drawn.tile_id) : visible;
  const sorted = [...others].sort((a, b) => {
    return normalizeTileKey(a.tile_key).localeCompare(normalizeTileKey(b.tile_key));
  });
  const sameKey = selectedId
    ? normalizeTileKey(
        tiles.find((t) => t.tile_id === selectedId)?.tile_key ?? "",
      )
    : null;

  const render = (t: TileRef, isDrawn: boolean) => (
    <TileFace
      key={t.tile_id}
      tileKey={t.tile_key}
      size="lg"
      selected={selectedId === t.tile_id}
      disabled={restrictedIds.has(t.tile_id)}
      highlight={!!sameKey && normalizeTileKey(t.tile_key) === sameKey}
      drawn={isDrawn}
      onClick={() => onSelect(t.tile_id)}
      onDoubleClick={() => onDoubleDiscard(t.tile_id)}
    />
  );

  return (
    <div className="my-hand">
      {sorted.map((t) => render(t, false))}
      {drawn ? render(drawn, true) : null}
    </div>
  );
}
