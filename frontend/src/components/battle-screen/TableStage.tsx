import { useCallback, useRef, useState, type CSSProperties, type ReactNode } from 'react';

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

import { MatchStatusBar } from './scene/MatchStatusBar';
import { MotionLayer } from './scene/MotionLayer';
import { buildTableSceneModel } from './scene/sceneModel';
import { SeatZone } from './scene/SeatZone';
import { QuickChatCluster } from './scene/QuickChatCluster';
import { IntroductionLayer } from './scene/IntroductionLayer';
import { TableChrome } from './scene/TableChrome';
import type { TableStagePlayer } from './scene/types';
import { useBattleViewport } from './scene/useBattleViewport';

export type { TableStagePlayer } from './scene/types';

const PLAYER_COLOR_SLOT_COUNT = 4;
const CENTER_CAPSULE_SPOTLIGHT_OFFSET =
  'calc(var(--table-stage-center-capsule-h) / 2 + (var(--table-stage-river-base-height) * var(--table-stage-spotlight-scale)) / 2 + var(--table-stage-spotlight-gap) * 2)';
const CENTER_CAPSULE_SPOTLIGHT_OFFSET_HORIZONTAL =
  'calc(var(--table-stage-center-capsule-w) / 2 + (var(--table-stage-river-base-height) * var(--table-stage-spotlight-scale)) / 2 + (var(--table-stage-spotlight-gap) * 2))';

type CenterStatusSize = {
  width: number;
  height: number;
};

function getPlayerIdentityKey(player: TableStagePlayer) {
  if (typeof player.userId === 'number') {
    return `user:${player.userId}`;
  }

  const name = player.name.trim();
  if (name) {
    return `name:${name}`;
  }

  return `seat:${player.absoluteSeat ?? player.seat}`;
}

function withPlayerColorSlots(players: TableStagePlayer[], colorSlotsByPlayer: Map<string, number>) {
  const usedSlots = new Set<number>();

  return players.map((player) => {
    const identityKey = getPlayerIdentityKey(player);
    let colorSlot = colorSlotsByPlayer.get(identityKey);

    if (typeof colorSlot !== 'number' || usedSlots.has(colorSlot)) {
      colorSlot = Array.from({ length: PLAYER_COLOR_SLOT_COUNT }, (_, index) => index)
        .find((slot) => !usedSlots.has(slot)) ?? (colorSlotsByPlayer.size % PLAYER_COLOR_SLOT_COUNT);
      colorSlotsByPlayer.set(identityKey, colorSlot);
    }

    usedSlots.add(colorSlot);
    return { ...player, colorSlot };
  });
}

interface TableStageProps {
  discards: Record<Seat, string[]>;
  selectedTileCode?: string | null;
  activeSeat: Seat;
  actionIndicatorSeat?: Seat | null;
  shouldDebounceWaitingStatus?: boolean;
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
  onOpenInviteDialog?: () => void;
  onCycleTheme?: () => void;
  onAction?: (actionId: BattleActionView['id']) => void;
  onAddBot?: () => void;
  onRemoveBot?: () => void;
  onQuickChat?: (targetSeat: number, emoji: QuickChatEmoji) => void;
  onPointGesture?: (targetSeat: number) => void;
  onDecreaseTileScale?: () => void;
  onIncreaseTileScale?: () => void;
  isBgmEnabled?: boolean;
  onToggleBgm?: () => void;
  isVoiceEnabled?: boolean;
  onToggleVoice?: () => void;
  isBotTakeoverEnabled?: boolean;
  onToggleBotTakeover?: (enabled: boolean) => void;
  isPlaying?: boolean;
  children?: ReactNode;
}

export function TableStage({
  discards,
  selectedTileCode = null,
  activeSeat,
  actionIndicatorSeat = null,
  shouldDebounceWaitingStatus = false,
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
  onOpenInviteDialog,
  onCycleTheme,
  onAction,
  onAddBot,
  onRemoveBot,
  onQuickChat,
  onPointGesture,
  onDecreaseTileScale: _onDecreaseTileScale,
  onIncreaseTileScale: _onIncreaseTileScale,
  isBgmEnabled = false,
  onToggleBgm,
  isVoiceEnabled = true,
  onToggleVoice,
  isBotTakeoverEnabled = false,
  onToggleBotTakeover,
  isPlaying = false,
  children,
}: TableStageProps) {
  const containerRef = useRef<HTMLElement | null>(null);
  const playerColorSlotsRef = useRef(new Map<string, number>());
  const [centerStatusSize, setCenterStatusSize] = useState<CenterStatusSize | null>(null);
  const viewport = useBattleViewport(containerRef);
  const shouldShowAspectRatioPrompt = viewport.width < viewport.height;
  const playersWithColorSlots = withPlayerColorSlots(players, playerColorSlotsRef.current);
  const localPlayerAbsoluteSeat = sceneLocalAbsoluteSeat(playersWithColorSlots);
  const scene = buildTableSceneModel({
    viewport,
    players: playersWithColorSlots,
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
  const handleCenterStatusSizeChange = useCallback((nextSize: CenterStatusSize) => {
    const roundedSize = {
      width: Math.round(nextSize.width),
      height: Math.round(nextSize.height),
    };

    setCenterStatusSize((currentSize) => {
      if (
        currentSize &&
        currentSize.width === roundedSize.width &&
        currentSize.height === roundedSize.height
      ) {
        return currentSize;
      }

      return roundedSize;
    });
  }, []);
  const stageStyle = {
    ...scene.stageStyle,
    ...(centerStatusSize
      ? {
          '--table-stage-center-capsule-w': `${centerStatusSize.width}px`,
          '--table-stage-center-capsule-h': `${centerStatusSize.height}px`,
          '--table-stage-spotlight-offset': CENTER_CAPSULE_SPOTLIGHT_OFFSET,
          '--table-stage-spotlight-offset-horizontal': CENTER_CAPSULE_SPOTLIGHT_OFFSET_HORIZONTAL,
        }
      : {}),
  } as CSSProperties;

  return (
    <section
      ref={containerRef}
      className="table-stage"
      aria-label="Mahjong table"
      style={stageStyle}
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
            onOpenInviteDialog={onOpenInviteDialog}
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
          <MatchStatusBar
            remainingCount={remainingTileCount}
            actionSeat={actionIndicatorSeat}
            dealerSelection={dealerSelection}
            deadlineAt={deadlineAt}
            isAmbiguous={!actionIndicatorSeat && !!remainingTileCount}
            shouldDebounceWaiting={shouldDebounceWaitingStatus}
            onSizeChange={handleCenterStatusSizeChange}
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
              onPlayerInfoDoubleClick={(targetSeat) => {
                if (targetSeat !== localPlayerAbsoluteSeat) {
                  onPointGesture?.(targetSeat);
                }
              }}
            />
          ))}
          <QuickChatCluster
            localPlayerName={scene.localPlayer?.name ?? '本家'}
            localPlayerAbsoluteSeat={scene.localPlayer?.absoluteSeat ?? null}
            onQuickChat={onQuickChat}
          />
          <IntroductionLayer
            players={playersWithColorSlots}
            discards={discards}
            isPlaying={isPlaying}
            actionEffect={actionEffect}
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

function sceneLocalAbsoluteSeat(players: TableStagePlayer[]) {
  return players.find((player) => player.isLocal)?.absoluteSeat ?? null;
}
