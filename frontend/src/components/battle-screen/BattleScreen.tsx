import { useState } from 'react';

import type { BattleActionId, BattleViewModel, ClaimActionId } from '../../types/match';
import { ActionEffectsOverlay } from './ActionEffectsOverlay';
import { AmbientOverlay } from './AmbientOverlay';
import { BottomActionDock } from './BottomActionDock';
import { FloatingRoomControls } from './FloatingRoomControls';
import { ResultOverlay } from './ResultOverlay';
import { TableStage } from './TableStage';

interface BattleScreenProps {
  viewModel: BattleViewModel;
  onTileSelect: (tileId: string) => void;
  onTileDoubleClick: (tileId: string) => void;
  onClaimCandidateSelect: (actionId: ClaimActionId, tileIds: string[]) => void;
  onClaimCandidateActivate: (actionId: ClaimActionId, tileIds: string[]) => void;
  onAction: (actionId: BattleActionId) => void;
  onCopyTableCode: () => void;
  onLeaveTable: () => void;
}

const REMOTE_PLAYER_ORDER = ['left', 'top', 'right'] as const;
const DEFAULT_TABLE_TILE_SCALE = 1.12;
const TABLE_TILE_SCALE_STEP = 0.06;
const MIN_TABLE_TILE_SCALE = 0.88;
const MAX_TABLE_TILE_SCALE = 1.3;

export function BattleScreen({
  viewModel,
  onTileSelect,
  onTileDoubleClick,
  onClaimCandidateSelect,
  onClaimCandidateActivate,
  onAction,
  onCopyTableCode,
  onLeaveTable,
}: BattleScreenProps) {
  const [tableTileScale, setTableTileScale] = useState(DEFAULT_TABLE_TILE_SCALE);
  const orderedPlayers = [
    viewModel.players.find((item) => item.seat === 'bottom'),
    ...REMOTE_PLAYER_ORDER.map((seat) => viewModel.players.find((item) => item.seat === seat)),
  ].filter((player): player is NonNullable<typeof player> => Boolean(player));
  const roomActions = viewModel.actions.filter((action) => ROOM_CONTROL_ACTION_IDS.includes(action.id));
  const battleActions = viewModel.actions.filter((action) => !ROOM_CONTROL_ACTION_IDS.includes(action.id));
  const canDecreaseTableTileScale = tableTileScale > MIN_TABLE_TILE_SCALE;
  const canIncreaseTableTileScale = tableTileScale < MAX_TABLE_TILE_SCALE;

  function adjustTableTileScale(offset: number) {
    setTableTileScale((currentScale) => {
      const nextScale = Number((currentScale + offset).toFixed(2));

      return Math.min(MAX_TABLE_TILE_SCALE, Math.max(MIN_TABLE_TILE_SCALE, nextScale));
    });
  }

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
              settlementHands={viewModel.settlementHands}
              tileScale={tableTileScale}
              canDecreaseTileScale={canDecreaseTableTileScale}
              canIncreaseTileScale={canIncreaseTableTileScale}
              onDecreaseTileScale={() => adjustTableTileScale(-TABLE_TILE_SCALE_STEP)}
              onIncreaseTileScale={() => adjustTableTileScale(TABLE_TILE_SCALE_STEP)}
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
        claimCandidates={viewModel.claimCandidates}
        actions={battleActions}
        isElevated={viewModel.isActionDockElevated}
        promptCue={viewModel.promptCue}
        deadlineAt={viewModel.deadlineAt}
        onTileSelect={onTileSelect}
        onTileDoubleClick={onTileDoubleClick}
        onClaimCandidateSelect={onClaimCandidateSelect}
        onClaimCandidateActivate={onClaimCandidateActivate}
        onAction={onAction}
      />
    </main>
  );
}

const ROOM_CONTROL_ACTION_IDS: BattleActionId[] = ['ready', 'start_match', 'start_next_round', 'restart_match'];
