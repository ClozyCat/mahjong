import type { BattleActionId, BattleViewModel } from '../../types/match';
import { AmbientOverlay } from './AmbientOverlay';
import { BottomActionDock } from './BottomActionDock';
import { FloatingRoomControls } from './FloatingRoomControls';
import { PlayerRing } from './PlayerRing';
import { ResultOverlay } from './ResultOverlay';
import { StageBackground } from './StageBackground';
import { TableStage } from './TableStage';
import { TopMatchBar } from './TopMatchBar';
import { WindowFrame } from '../win98/WindowFrame';

interface BattleScreenProps {
  viewModel: BattleViewModel;
  onTileSelect: (tileId: string) => void;
  onAction: (actionId: BattleActionId) => void;
  onCopyTableCode: () => void;
  onLeaveTable: () => void;
}

const REMOTE_PLAYER_ORDER = ['left', 'top', 'right'] as const;

export function BattleScreen({ viewModel, onTileSelect, onAction, onCopyTableCode, onLeaveTable }: BattleScreenProps) {
  const localPlayer = viewModel.players.find((item) => item.isLocal) ?? null;
  const remotePlayers = REMOTE_PLAYER_ORDER.map((seat) => viewModel.players.find((item) => item.seat === seat)).filter(
    (player): player is NonNullable<typeof player> => Boolean(player),
  );
  const roomActions = viewModel.actions.filter((action) => ROOM_CONTROL_ACTION_IDS.includes(action.id));
  const battleActions = viewModel.actions.filter((action) => !ROOM_CONTROL_ACTION_IDS.includes(action.id));

  return (
    <main className="battle-screen">
      <StageBackground />
      <WindowFrame title="四风麻将客户端" status={`状态：${viewModel.topStatusLabel}`} className="battle-shell">
        <TopMatchBar
          tableCode={viewModel.tableCode}
          canLeaveTable={viewModel.canLeaveTable}
          phaseLabel={viewModel.phaseLabel}
          roundLabel={viewModel.roundLabel}
          scoreSummaryLabel={viewModel.scoreSummaryLabel}
          deadlineAt={viewModel.deadlineAt}
          topStatusLabel={viewModel.topStatusLabel}
          onCopyTableCode={onCopyTableCode}
          onLeaveTable={onLeaveTable}
        />
        <div className="battle-stage">
          <div className="battle-stage__halo" />
          <div className="battle-stage__player-row">
            {remotePlayers.map((player) => (
              <PlayerRing key={player.seat} player={player} />
            ))}
          </div>
          <div className="battle-stage__table-wrap">
            <TableStage
              discards={viewModel.discards}
              activeSeat={viewModel.activePlayerSeat}
              lastDiscard={viewModel.lastDiscard}
              lastDiscardSeat={viewModel.lastDiscardSeat}
              remainingTileCount={viewModel.remainingTileCount}
              promptText={viewModel.promptText}
              players={viewModel.players}
            />
          </div>
          <AmbientOverlay
            mode={viewModel.mode}
            promptText={viewModel.promptText}
            waitingControls={viewModel.waitingControls}
            toasts={viewModel.toasts}
          />
          <FloatingRoomControls actions={roomActions} onAction={onAction} />
          {viewModel.result ? <ResultOverlay result={viewModel.result} onAction={onAction} /> : null}
        </div>
        <BottomActionDock
          hand={viewModel.localHand}
          actions={battleActions}
          isElevated={viewModel.isActionDockElevated}
          waitingControls={viewModel.waitingControls}
          localPlayer={localPlayer}
          onTileSelect={onTileSelect}
          onAction={onAction}
        />
      </WindowFrame>
    </main>
  );
}

const ROOM_CONTROL_ACTION_IDS: BattleActionId[] = ['ready', 'start_match', 'start_next_round', 'restart_match'];
