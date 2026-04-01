import { useEffect, useState } from 'react';

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
const SETTLEMENT_PANEL_DELAY_MS = 420;
const MIN_BATTLE_VIEWPORT_WIDTH = 1280;
const MIN_BATTLE_VIEWPORT_HEIGHT = 720;
const MIN_BATTLE_VIEWPORT_RATIO = 16 / 9;

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
  const [viewportState, setViewportState] = useState(getBattleViewportState);
  const [isSettlementPanelReady, setIsSettlementPanelReady] = useState(true);
  const orderedPlayers = [
    viewModel.players.find((item) => item.seat === 'bottom'),
    ...REMOTE_PLAYER_ORDER.map((seat) => viewModel.players.find((item) => item.seat === seat)),
  ].filter((player): player is NonNullable<typeof player> => Boolean(player));
  const roomActions = viewModel.actions.filter((action) => ROOM_CONTROL_ACTION_IDS.includes(action.id));
  const battleActions = viewModel.actions.filter((action) => !ROOM_CONTROL_ACTION_IDS.includes(action.id));
  const canDecreaseTableTileScale = tableTileScale > MIN_TABLE_TILE_SCALE;
  const canIncreaseTableTileScale = tableTileScale < MAX_TABLE_TILE_SCALE;
  const shouldDelaySettlementPanel =
    Boolean(viewModel.result) && Boolean(viewModel.lastDiscard) && viewModel.result?.winType === 'draw';
  const visibleLastDiscard = shouldDelaySettlementPanel ? null : viewModel.lastDiscard;
  const visibleLastDiscardSeat = shouldDelaySettlementPanel ? null : viewModel.lastDiscardSeat;
  const visibleResult = isSettlementPanelReady ? viewModel.result : null;

  function adjustTableTileScale(offset: number) {
    setTableTileScale((currentScale) => {
      const nextScale = Number((currentScale + offset).toFixed(2));

      return Math.min(MAX_TABLE_TILE_SCALE, Math.max(MIN_TABLE_TILE_SCALE, nextScale));
    });
  }

  useEffect(() => {
    if (typeof window === 'undefined') {
      return undefined;
    }

    function handleResize() {
      setViewportState(getBattleViewportState());
    }

    window.addEventListener('resize', handleResize);

    return () => window.removeEventListener('resize', handleResize);
  }, []);

  useEffect(() => {
    if (!viewModel.result) {
      setIsSettlementPanelReady(true);
      return undefined;
    }

    if (!shouldDelaySettlementPanel) {
      setIsSettlementPanelReady(true);
      return undefined;
    }

    setIsSettlementPanelReady(false);
    const timer = window.setTimeout(() => {
      setIsSettlementPanelReady(true);
    }, SETTLEMENT_PANEL_DELAY_MS);

    return () => window.clearTimeout(timer);
  }, [shouldDelaySettlementPanel, viewModel.result]);

  return (
    <main className={`battle-screen ${viewportState.isSupported ? '' : 'battle-screen--viewport-blocked'}`}>
      <div className="battle-shell">
        <div className="battle-stage">
          <div className="battle-stage__halo" />
          <div className="battle-stage__table-wrap">
            <TableStage
              discards={viewModel.discards}
              activeSeat={viewModel.activePlayerSeat}
              lastDiscard={visibleLastDiscard}
              lastDiscardSeat={visibleLastDiscardSeat}
              remainingTileCount={viewModel.remainingTileCount}
              promptText={viewModel.promptText}
              promptCue={viewModel.promptCue}
              players={viewModel.players}
              settlementHands={viewModel.settlementHands}
              tileScale={tableTileScale}
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
          {visibleResult ? <ResultOverlay result={visibleResult} onAction={onAction} /> : null}
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
        tableTileScale={tableTileScale}
        canDecreaseTileScale={canDecreaseTableTileScale}
        canIncreaseTileScale={canIncreaseTableTileScale}
        onCopyTableCode={onCopyTableCode}
        onLeaveTable={onLeaveTable}
        onDecreaseTileScale={() => adjustTableTileScale(-TABLE_TILE_SCALE_STEP)}
        onIncreaseTileScale={() => adjustTableTileScale(TABLE_TILE_SCALE_STEP)}
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
      {viewportState.isSupported ? null : (
        <div className="battle-screen__viewport-guard" role="alert" aria-live="assertive">
          <div className="battle-screen__viewport-guard-card">
            <span className="battle-screen__viewport-guard-eyebrow">显示条件不足</span>
            <strong>请把浏览器窗口调整到大于 1280 x 720，且宽高比大于 16:9</strong>
            <p>
              当前可用区域为 {viewportState.width} x {viewportState.height}，宽高比 {viewportState.ratioLabel}。
            </p>
          </div>
        </div>
      )}
    </main>
  );
}

const ROOM_CONTROL_ACTION_IDS: BattleActionId[] = ['ready', 'start_match', 'start_next_round', 'restart_match'];

function getBattleViewportState() {
  if (typeof window === 'undefined') {
    return {
      width: 1920,
      height: 1080,
      ratioLabel: '1.78',
      isSupported: true,
    };
  }

  const width = window.innerWidth;
  const height = window.innerHeight;
  const ratio = height > 0 ? width / height : 0;

  return {
    width,
    height,
    ratioLabel: ratio.toFixed(2),
    isSupported:
      width > MIN_BATTLE_VIEWPORT_WIDTH &&
      height > MIN_BATTLE_VIEWPORT_HEIGHT &&
      ratio > MIN_BATTLE_VIEWPORT_RATIO,
  };
}
