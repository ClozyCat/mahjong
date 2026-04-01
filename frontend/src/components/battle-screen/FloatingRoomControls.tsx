import type { BattleActionView, PlayerView, WaitingControls } from '../../types/match';

interface FloatingRoomControlsProps {
  players: PlayerView[];
  actions: BattleActionView[];
  tableCode: string;
  canLeaveTable: boolean;
  phaseLabel: string;
  roundLabel: string;
  scoreSummaryLabel: string;
  deadlineAt: string | null;
  topStatusLabel: string;
  promptText: string | null;
  remainingTileCount?: number | null;
  waitingControls: WaitingControls | null;
  tableTileScale?: number;
  canDecreaseTileScale?: boolean;
  canIncreaseTileScale?: boolean;
  onCopyTableCode: () => void;
  onLeaveTable: () => void;
  onDecreaseTileScale?: () => void;
  onIncreaseTileScale?: () => void;
  onAction: (actionId: BattleActionView['id']) => void;
}

export function FloatingRoomControls({
  actions: _actions,
  canLeaveTable: _canLeaveTable,
  onLeaveTable: _onLeaveTable,
  onAction: _onAction,
}: FloatingRoomControlsProps) {
  return null;
}
