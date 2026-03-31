import type { BattleActionId, BattleViewModel } from '../../types/match';
import { ActionEffectsOverlay } from './ActionEffectsOverlay';
import { AmbientOverlay } from './AmbientOverlay';
import { BottomActionDock } from './BottomActionDock';
import { FloatingRoomControls } from './FloatingRoomControls';
import { ResultOverlay } from './ResultOverlay';
import { TableStage } from './TableStage';

interface BattleScreenProps {
  viewModel: BattleViewModel;
  onTileSelect: (tileId: string) => void;
  onAction: (actionId: BattleActionId) => void;
  onCopyTableCode: () => void;
  onLeaveTable: () => void;
}

const REMOTE_PLAYER_ORDER = ['left', 'top', 'right'] as const;

export function BattleScreen({ viewModel, onTileSelect, onAction, onCopyTableCode, onLeaveTable }: BattleScreenProps) {
  const orderedPlayers = [
    viewModel.players.find((item) => item.seat === 'bottom'),
    ...REMOTE_PLAYER_ORDER.map((seat) => viewModel.players.find((item) => item.seat === seat)),
  ].filter((player): player is NonNullable<typeof player> => Boolean(player));
  const roomActions = viewModel.actions.filter((action) => ROOM_CONTROL_ACTION_IDS.includes(action.id));
  const battleActions = viewModel.actions.filter((action) => !ROOM_CONTROL_ACTION_IDS.includes(action.id));

  return (
    <main className="battle-screen">
      <div className="battle-shell">
        <div className="battle-stage">
          <div className="battle-stage__halo" />
          <div className="battle-stage__table-wrap">
            <TableStage
              discards={viewModel.discards}
              activeSeat={viewModel.activePlayerSeat}
              lastDiscard={viewModel.lastDiscard}
              lastDiscardSeat={viewModel.lastDiscardSeat}
              remainingTileCount={viewModel.remainingTileCount}
              promptText={viewModel.promptText}
              promptCue={viewModel.promptCue}
              players={viewModel.players}
            />
          </div>
          <ActionEffectsOverlay
            actionEffect={viewModel.actionEffect}
            celebrationEffect={viewModel.celebrationEffect}
            drawnTileId={viewModel.drawnTileId}
          />
          <AmbientOverlay
            mode={viewModel.mode}
            promptText={viewModel.promptText}
            waitingControls={viewModel.waitingControls}
          />
          {viewModel.result ? <ResultOverlay result={viewModel.result} onAction={onAction} /> : null}
        </div>
      </div>
      <FloatingRoomControls
        players={orderedPlayers}
        actions={roomActions}
        tableCode={viewModel.tableCode}
        canLeaveTable={viewModel.canLeaveTable}
        phaseLabel={viewModel.phaseLabel}
        roundLabel={viewModel.roundLabel}
        scoreSummaryLabel={viewModel.scoreSummaryLabel}
        deadlineAt={viewModel.deadlineAt}
        topStatusLabel={viewModel.topStatusLabel}
        promptText={viewModel.promptText}
        remainingTileCount={viewModel.remainingTileCount}
        waitingControls={viewModel.waitingControls}
        onCopyTableCode={onCopyTableCode}
        onLeaveTable={onLeaveTable}
        onAction={onAction}
      />
      <BottomActionDock
        hand={viewModel.localHand}
        actions={battleActions}
        isElevated={viewModel.isActionDockElevated}
        promptCue={viewModel.promptCue}
        deadlineAt={viewModel.deadlineAt}
        onTileSelect={onTileSelect}
        onAction={onAction}
      />
    </main>
  );
}

const ROOM_CONTROL_ACTION_IDS: BattleActionId[] = ['ready', 'start_match', 'start_next_round', 'restart_match'];
