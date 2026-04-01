import type { ActionEffectView, CelebrationEffectView } from '../../types/match';

interface ActionEffectsOverlayProps {
  actionEffect: ActionEffectView | null;
  celebrationEffect: CelebrationEffectView | null;
  drawnTileId: string | null;
}

export function ActionEffectsOverlay({
  actionEffect: _actionEffect,
  celebrationEffect: _celebrationEffect,
  drawnTileId: _drawnTileId,
}: ActionEffectsOverlayProps) {
  return null;
}
