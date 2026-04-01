import { useEffect, useRef, useState } from 'react';

import type { BattleActionId, BattleViewModel, ClaimActionId } from '../../types/match';
import type { ThemeId } from '../../lib/themes';
import { AmbientOverlay } from './AmbientOverlay';
import { BottomActionDock } from './BottomActionDock';
import { ResultOverlay } from './ResultOverlay';
import { TableStage } from './TableStage';

interface BattleScreenProps {
  viewModel: BattleViewModel;
  themeId: ThemeId;
  themeLabel: string;
  onCycleTheme: () => void;
  onTileSelect: (tileId: string) => void;
  onTileDoubleClick: (tileId: string) => void;
  onClaimCandidateSelect: (actionId: ClaimActionId, tileIds: string[]) => void;
  onClaimCandidateActivate: (actionId: ClaimActionId, tileIds: string[]) => void;
  onAction: (actionId: BattleActionId) => void;
  onCopyTableCode: () => void;
  onLeaveTable: () => void;
}

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
  themeId,
  themeLabel,
  onCycleTheme,
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
  const [consumedActionEffect, setConsumedActionEffect] = useState(viewModel.actionEffect);
  const consumedActionEffectKeyRef = useRef<string | null>(viewModel.actionEffect?.key ?? null);
  const preMatchActions = viewModel.actions.filter((action) => PRE_MATCH_ACTION_IDS.includes(action.id));
  const battleActions = viewModel.actions.filter((action) => !TABLE_ONLY_ACTION_IDS.includes(action.id));
  const occupiedSeatCount = viewModel.waitingControls?.occupiedSeats ?? viewModel.players.length;
  const canDecreaseTableTileScale = tableTileScale > MIN_TABLE_TILE_SCALE;
  const canIncreaseTableTileScale = tableTileScale < MAX_TABLE_TILE_SCALE;
  const shouldReturnLastDiscardToRiver =
    Boolean(viewModel.result) &&
    Boolean(viewModel.lastDiscard) &&
    (viewModel.result?.winType === 'draw' || viewModel.result?.winType === 'self_draw');
  const shouldDelaySettlementPanel =
    Boolean(viewModel.result) && Boolean(viewModel.lastDiscard) && viewModel.result?.winType === 'draw';
  const visibleLastDiscard = shouldReturnLastDiscardToRiver ? null : viewModel.lastDiscard;
  const visibleLastDiscardSeat = shouldReturnLastDiscardToRiver ? null : viewModel.lastDiscardSeat;
  const visibleResult = isSettlementPanelReady ? viewModel.result : null;

  function adjustTableTileScale(offset: number) {
    setTableTileScale((currentScale) => {
      const nextScale = Number((currentScale + offset).toFixed(2));

      return Math.min(MAX_TABLE_TILE_SCALE, Math.max(MIN_TABLE_TILE_SCALE, nextScale));
    });
  }

  useEffect(() => {
    const nextActionEffect = viewModel.actionEffect;
    if (!nextActionEffect?.key) {
      return;
    }

    if (consumedActionEffectKeyRef.current === nextActionEffect.key) {
      return;
    }

    consumedActionEffectKeyRef.current = nextActionEffect.key;
    setConsumedActionEffect(nextActionEffect);
  }, [viewModel.actionEffect]);

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
              actionIndicatorSeat={viewModel.actionIndicatorSeat}
              lastDiscard={visibleLastDiscard}
              lastDiscardSeat={visibleLastDiscardSeat}
              settlementWinnerSeat={viewModel.result?.winnerSeat ?? null}
              settlementWinType={viewModel.result?.winType ?? null}
              remainingTileCount={viewModel.remainingTileCount}
              promptText={viewModel.promptText}
              promptCue={viewModel.promptCue}
              actionEffect={consumedActionEffect}
              players={viewModel.players}
              settlementHands={viewModel.settlementHands}
              tableCode={viewModel.tableCode}
              roundLabel={viewModel.roundLabel}
              phaseLabel={viewModel.phaseLabel}
              occupiedSeatCount={occupiedSeatCount}
              seatCapacity={4}
              preMatchActions={viewModel.waitingControls ? preMatchActions : []}
              tileScale={tableTileScale}
              canDecreaseTileScale={canDecreaseTableTileScale}
              canIncreaseTileScale={canIncreaseTableTileScale}
              canLeaveTable={viewModel.canLeaveTable}
              themeId={themeId}
              themeLabel={themeLabel}
              onLeaveTable={onLeaveTable}
              onCycleTheme={onCycleTheme}
              onAction={onAction}
              onDecreaseTileScale={() => adjustTableTileScale(-TABLE_TILE_SCALE_STEP)}
              onIncreaseTileScale={() => adjustTableTileScale(TABLE_TILE_SCALE_STEP)}
            />
          </div>
          <AmbientOverlay
            mode={viewModel.mode}
            promptText={viewModel.promptText}
            waitingControls={viewModel.waitingControls}
            canLeaveTable={viewModel.canLeaveTable}
            onLeaveTable={onLeaveTable}
          />
          {visibleResult ? <ResultOverlay result={visibleResult} onAction={onAction} /> : null}
        </div>
      </div>
      <BottomActionDock
        hand={viewModel.localHand}
        claimCandidates={viewModel.claimCandidates}
        actions={battleActions}
        isElevated={viewModel.isActionDockElevated}
        isWaitingForMatchStart={Boolean(viewModel.waitingControls)}
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

const PRE_MATCH_ACTION_IDS: BattleActionId[] = ['ready', 'start_match'];
const HIDDEN_TABLE_ACTION_IDS: BattleActionId[] = ['start_next_round', 'restart_match'];
const TABLE_ONLY_ACTION_IDS: BattleActionId[] = [...PRE_MATCH_ACTION_IDS, ...HIDDEN_TABLE_ACTION_IDS];

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
