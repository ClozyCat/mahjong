import { useRef, type ReactNode } from 'react';

import type { ThemeId } from '../../lib/themes';
import type {
  ActionEffectView,
  BattleActionView,
  BattlePromptView,
  DealerSelectionView,
  QuickChatEmoji,
  QuickChatEventView,
  Seat,
  SystemBroadcastEventView,
} from '../../types/match';

import { CenterIndicator } from './scene/CenterIndicator';
import { MotionLayer } from './scene/MotionLayer';
import { buildTableSceneModel } from './scene/sceneModel';
import { SeatZone } from './scene/SeatZone';
import { QuickChatCluster } from './scene/QuickChatCluster';
import { TableChrome } from './scene/TableChrome';
import type { TableStagePlayer } from './scene/types';
import { useBattleViewport } from './scene/useBattleViewport';

export type { TableStagePlayer } from './scene/types';

interface TableStageProps {
  discards: Record<Seat, string[]>;
  selectedTileCode?: string | null;
  activeSeat: Seat;
  actionIndicatorSeat?: Seat | null;
  lastDiscard: string | null;
  lastDiscardSeat?: Seat | null;
  settlementWinnerSeat?: Seat | null;
  settlementWinnerSeats?: Seat[];
  settlementWinType?: string | null;
  settlementWinTypeLabel?: string | null;
  centerStatusText?: string | null;
  remainingTileCount?: number | null;
  promptText: string | null;
  promptCue?: BattlePromptView | null;
  dealerSelection?: DealerSelectionView | null;
  deadlineAt?: string | null;
  actionEffect?: ActionEffectView | null;
  quickChatEvent?: QuickChatEventView | null;
  systemBroadcastEvent?: SystemBroadcastEventView | null;
  players?: TableStagePlayer[];
  settlementCenterCalloutLabel?: string | null;
  tableCode?: string;
  roundLabel?: string;
  phaseLabel?: string;
  occupiedSeatCount?: number;
  seatCapacity?: number;
  preMatchActions?: BattleActionView[];
  isWaitingForMatchStart?: boolean;
  botCount?: number;
  canAddBot?: boolean;
  canRemoveBot?: boolean;
  tileScale?: number;
  canDecreaseTileScale?: boolean;
  canIncreaseTileScale?: boolean;
  canLeaveTable?: boolean;
  themeId?: ThemeId;
  themeLabel?: string;
  onLeaveTable?: () => void;
  onCycleTheme?: () => void;
  onAction?: (actionId: BattleActionView['id']) => void;
  onAddBot?: () => void;
  onRemoveBot?: () => void;
  onQuickChat?: (targetSeat: number, emoji: QuickChatEmoji) => void;
  onDecreaseTileScale?: () => void;
  onIncreaseTileScale?: () => void;
  isBgmEnabled?: boolean;
  onToggleBgm?: () => void;
  isVoiceEnabled?: boolean;
  onToggleVoice?: () => void;
  isBotTakeoverEnabled?: boolean;
  onToggleBotTakeover?: (enabled: boolean) => void;
  children?: ReactNode;
}

export function TableStage({
  discards,
  selectedTileCode = null,
  activeSeat,
  actionIndicatorSeat = null,
  lastDiscard,
  lastDiscardSeat = null,
  settlementWinnerSeat = null,
  settlementWinnerSeats = [],
  settlementWinType = null,
  settlementWinTypeLabel = null,
  centerStatusText = null,
  remainingTileCount = null,
  promptText: _promptText,
  promptCue = null,
  dealerSelection = null,
  deadlineAt = null,
  actionEffect = null,
  quickChatEvent = null,
  systemBroadcastEvent = null,
  players = [],
  settlementCenterCalloutLabel = null,
  tableCode = '',
  roundLabel = '',
  phaseLabel = '',
  occupiedSeatCount,
  seatCapacity = 4,
  preMatchActions = [],
  isWaitingForMatchStart = false,
  botCount = 0,
  canAddBot = false,
  canRemoveBot = false,
  tileScale = 1,
  canDecreaseTileScale: _canDecreaseTileScale = false,
  canIncreaseTileScale: _canIncreaseTileScale = false,
  canLeaveTable = false,
  themeId = 'tian-shui-bi',
  themeLabel = '天水碧',
  onLeaveTable,
  onCycleTheme,
  onAction,
  onAddBot,
  onRemoveBot,
  onQuickChat,
  onDecreaseTileScale: _onDecreaseTileScale,
  onIncreaseTileScale: _onIncreaseTileScale,
  isBgmEnabled = false,
  onToggleBgm,
  isVoiceEnabled = true,
  onToggleVoice,
  isBotTakeoverEnabled = false,
  onToggleBotTakeover,
  children,
}: TableStageProps) {
  const containerRef = useRef<HTMLElement | null>(null);
  const viewport = useBattleViewport(containerRef);
  const shouldShowAspectRatioPrompt = viewport.width < viewport.height;
  const scene = buildTableSceneModel({
    viewport,
    players,
    tileScale,
    occupiedSeatCount,
    seatCapacity,
    isWaitingForMatchStart,
    roundLabel,
    phaseLabel,
  });
  const hasSpotlightDiscard = Boolean(lastDiscard && lastDiscardSeat);
  const centerPrimaryText =
    centerStatusText ?? (typeof remainingTileCount === 'number' ? `剩余 ${remainingTileCount} 张` : '等待开局');

  return (
    <section
      ref={containerRef}
      className="table-stage"
      aria-label="Mahjong table"
      style={scene.stageStyle}
      data-layout={scene.layoutId}
      data-fx={scene.effectMode}
      data-center-status={centerPrimaryText}
    >
      <div className="table-stage__frame">
        <div className="table-stage__core">
          <TableChrome
            tableCode={tableCode}
            resolvedOccupiedSeatCount={scene.resolvedOccupiedSeatCount}
            seatCapacity={seatCapacity}
            themeId={themeId}
            themeLabel={themeLabel}
            tableSummary={scene.tableSummary}
            preMatchActions={preMatchActions}
            botCount={botCount}
            canAddBot={canAddBot}
            canRemoveBot={canRemoveBot}
            canLeaveTable={canLeaveTable}
            onLeaveTable={onLeaveTable}
            onCycleTheme={onCycleTheme}
            onAction={onAction}
            onAddBot={onAddBot}
            onRemoveBot={onRemoveBot}
            isBgmEnabled={isBgmEnabled}
            onToggleBgm={onToggleBgm}
            isVoiceEnabled={isVoiceEnabled}
            onToggleVoice={onToggleVoice}
            isBotTakeoverEnabled={isBotTakeoverEnabled}
            onToggleBotTakeover={onToggleBotTakeover}
          />
          <CenterIndicator
            remainingCount={remainingTileCount}
            actionSeat={actionIndicatorSeat}
            dealerSelection={dealerSelection}
            deadlineAt={deadlineAt}
            isAmbiguous={!actionIndicatorSeat && !!remainingTileCount}
          />
          {scene.seats.map((seatScene) => (
            <SeatZone
              key={`seat-zone-${seatScene.seat}`}
              scene={seatScene}
              discards={discards[seatScene.seat]}
              activeSeat={activeSeat}
              lastDiscardSeat={lastDiscardSeat}
              selectedTileCode={selectedTileCode}
              hasSpotlightDiscard={hasSpotlightDiscard}
            />
          ))}
          <QuickChatCluster
            localPlayerName={scene.localPlayer?.name ?? '本家'}
            localPlayerAbsoluteSeat={scene.localPlayer?.absoluteSeat ?? null}
            onQuickChat={onQuickChat}
          />
          <MotionLayer
            discards={discards}
            selectedTileCode={selectedTileCode}
            lastDiscard={lastDiscard}
            lastDiscardSeat={lastDiscardSeat}
            settlementWinnerSeat={settlementWinnerSeat}
            settlementWinnerSeats={settlementWinnerSeats}
            settlementWinType={settlementWinType}
            settlementWinTypeLabel={settlementWinTypeLabel}
            settlementCenterCalloutLabel={settlementCenterCalloutLabel}
            promptCue={promptCue}
            actionEffect={actionEffect}
            quickChatEvent={quickChatEvent}
            systemBroadcastEvent={systemBroadcastEvent}
          />
          {shouldShowAspectRatioPrompt ? (
            <div
              className="table-stage__aspect-ratio-prompt"
              role="alert"
              aria-live="assertive"
            >
              <div className="table-stage__aspect-ratio-panel">
                <strong>请旋转屏幕或调整窗口比例</strong>
                <span>当前牌桌需要宽度大于或等于高度的画面比例。</span>
              </div>
            </div>
          ) : null}
        </div>
      </div>
      {children}
    </section>
  );
}
