import type { CSSProperties } from 'react';
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';

import type {
  BackendActionType,
  BattleActionView,
  BattlePromptView,
  BattleViewModel,
  ClaimActionId,
} from '../../types/match';
import { MahjongTile } from './MahjongTile';
import { getFanLabel, getFanGuideEntry } from './fanGuide';

interface BottomActionDockProps {
  hand: BattleViewModel['localHand'];
  selectedTileCode?: string | null;
  handInsight?: BattleViewModel['handInsight'];
  claimCandidates: BattleViewModel['claimCandidates'];
  actions: BattleActionView[];
  isElevated: boolean;
  isWaitingForMatchStart?: boolean;
  isHandInteractionDisabled?: boolean;
  isSpectator?: boolean;
  spectatorFocusName?: string | null;
  promptCue: BattlePromptView | null;
  deadlineAt: string | null;
  onSwitchPerspective?: () => void;
  onTileSelect: (tileId: string) => void;
  onTileDoubleClick: (tileId: string) => void;
  onClaimCandidateSelect: (actionId: ClaimActionId, tileIds: string[]) => void;
  onClaimCandidateActivate: (actionId: ClaimActionId, tileIds: string[]) => void;
  onAction: (actionId: BattleActionView['id']) => void;
}

export function BottomActionDock({
  hand,
  selectedTileCode = null,
  handInsight = null,
  claimCandidates,
  actions,
  isWaitingForMatchStart = false,
  isHandInteractionDisabled = false,
  isSpectator = false,
  spectatorFocusName = null,
  promptCue,
  deadlineAt,
  onSwitchPerspective,
  onTileSelect,
  onTileDoubleClick,
  onClaimCandidateSelect,
  onClaimCandidateActivate,
  onAction,
}: BottomActionDockProps) {
  const [isHandInsightPopoverHovered, setIsHandInsightPopoverHovered] = useState(false);
  const [isHandInsightPopoverPinned, setIsHandInsightPopoverPinned] = useState(false);
  const [isHandInsightPopoverHorizontal, setIsHandInsightPopoverHorizontal] = useState(false);
  const handInsightPopoverRef = useRef<HTMLDivElement | null>(null);
  const handCount = hand.length;
  const hasDrawnTile = hand.some((tile) => tile.isDrawn || tile.isReplacementDrawn);
  const layoutHandCount = useMemo(() =>
    handCount > 0 ? ACTIVE_HAND_LAYOUT_COUNT : isWaitingForMatchStart ? WAITING_HAND_PLACEHOLDER_COUNT : 1
  , [handCount, isWaitingForMatchStart]);
  const layoutDrawnGapCount = handCount > 0 ? 1 : 0;

  const dockStyle = useMemo(() => ({
    '--action-dock-hand-count': `${handCount}`,
    '--action-dock-effective-hand-count': `${Math.max(handCount, 1)}`,
    '--action-dock-gap-count': `${Math.max(handCount - 1, 0)}`,
    '--action-dock-drawn-gap-count': hasDrawnTile ? '1' : '0',
    '--action-dock-layout-hand-count': `${layoutHandCount}`,
    '--action-dock-effective-layout-hand-count': `${Math.max(layoutHandCount, 1)}`,
    '--action-dock-layout-gap-count': `${Math.max(layoutHandCount - 1, 0)}`,
    '--action-dock-layout-drawn-gap-count': `${layoutDrawnGapCount}`,
  }), [handCount, hasDrawnTile, layoutDrawnGapCount, layoutHandCount]) as CSSProperties;

  const visibleActions = useMemo(() => actions
    .filter((action) => {
      if (!action.enabled) {
        return false;
      }

      if (!promptCue) {
        return true;
      }

      return promptCue.actionIds.includes(action.id as BackendActionType);
    })
    .sort(
      (left, right) =>
        (ACTION_PRIORITY[left.id] ?? Number.MAX_SAFE_INTEGER) -
        (ACTION_PRIORITY[right.id] ?? Number.MAX_SAFE_INTEGER),
    ), [actions, promptCue]);

  const hasHandInsightContent = useMemo(() => 
    Boolean(handInsight?.isTenpai || handInsight?.winningFans.length)
  , [handInsight]);
  
  const isHandInsightPopoverOpen =
    hasHandInsightContent && (isHandInsightPopoverHovered || isHandInsightPopoverPinned);
  
  const displayedWinningFans = useMemo(() => 
    handInsight ? getDisplayedWinningFans(handInsight) : []
  , [handInsight]);

  useEffect(() => {
    if (!isHandInsightPopoverPinned) {
      return undefined;
    }

    function handlePointerDown(event: PointerEvent) {
      if (!handInsightPopoverRef.current?.contains(event.target as Node)) {
        setIsHandInsightPopoverPinned(false);
      }
    }

    window.addEventListener('pointerdown', handlePointerDown);
    return () => window.removeEventListener('pointerdown', handlePointerDown);
  }, [isHandInsightPopoverPinned]);

  useEffect(() => {
    if (handInsight) {
      return;
    }

    setIsHandInsightPopoverHovered(false);
    setIsHandInsightPopoverPinned(false);
  }, [handInsight]);

  useLayoutEffect(() => {
    if (!isHandInsightPopoverOpen) {
      setIsHandInsightPopoverHorizontal(false);
      return undefined;
    }

    function updatePopoverLayout() {
      const popover = handInsightPopoverRef.current?.querySelector<HTMLElement>('.action-dock__ready-hand-popover');

      if (!popover) {
        return;
      }

      const maxNaturalHeight = window.innerHeight * 0.75;
      setIsHandInsightPopoverHorizontal(measureNaturalPopoverHeight(popover) > maxNaturalHeight);
    }

    updatePopoverLayout();
    window.addEventListener('resize', updatePopoverLayout);
    return () => window.removeEventListener('resize', updatePopoverLayout);
  }, [displayedWinningFans.length, handInsight?.waits.length, isHandInsightPopoverOpen]);

  const handInsightControl = hasHandInsightContent && handInsight ? (
    <div
      ref={handInsightPopoverRef}
      className="action-dock__ready-hand-anchor"
      onMouseEnter={() => setIsHandInsightPopoverHovered(true)}
      onMouseLeave={() => setIsHandInsightPopoverHovered(false)}
    >
      <button
        type="button"
        className={[
          'action-dock__ready-hand-trigger',
          'action-dock__ready-hand-trigger--tenpai',
          isHandInsightPopoverOpen ? 'action-dock__ready-hand-trigger--open' : '',
        ]
          .filter(Boolean)
          .join(' ')}
        aria-label={getHandInsightTriggerLabel(handInsight)}
        aria-expanded={isHandInsightPopoverOpen}
        onClick={() => setIsHandInsightPopoverPinned((current) => !current)}
        onContextMenu={(event) => {
          event.preventDefault();
          setIsHandInsightPopoverPinned((current) => !current);
        }}
      >
        i
      </button>
      {isHandInsightPopoverOpen ? (
        <section 
          className={[
            'action-dock__ready-hand-popover',
            isHandInsightPopoverPinned ? 'action-dock__ready-hand-popover--pinned' : '',
            isHandInsightPopoverHorizontal ? 'action-dock__ready-hand-popover--horizontal' : '',
          ].filter(Boolean).join(' ')}
          aria-label={getHandInsightPopoverLabel(handInsight)}
        >
          {handInsight.waits.length > 0 ? (
            <div className="action-dock__hand-insight-section">
              <strong className="action-dock__hand-insight-title">
                {handInsight.source === 'selected_discard' ? '打出后将听' : '正在听'}
              </strong>
              <div className="action-dock__ready-hand-list" role="list">
                {handInsight.waits.map((wait) => (
                  <div key={wait.code} className="action-dock__ready-hand-row" role="listitem">
                    <div className="action-dock__ready-hand-tile">
                      <MahjongTile
                        code={wait.code}
                        variant="discard"
                        relatedTileCode={selectedTileCode}
                        className="action-dock__ready-hand-preview-tile"
                      />
                    </div>
                    <strong>{wait.availableCount}</strong>
                  </div>
                ))}
              </div>
            </div>
          ) : null}
          <div className="action-dock__hand-insight-section">
            <strong className="action-dock__hand-insight-title">和牌番型</strong>
            {displayedWinningFans.length > 0 ? (
              <div
                className="action-dock__hand-insight-winning-fans"
                role="list"
                aria-label="和牌番型列表"
              >
                {displayedWinningFans.map((item) => (
                  <HandInsightWinningFanItem
                    key={item.fanKey}
                    item={item}
                    isPinned={isHandInsightPopoverPinned}
                  />
                ))}
              </div>
            ) : (
              <div className="action-dock__hand-insight-empty">暂无和牌番型</div>
            )}
          </div>
        </section>
      ) : null}
    </div>
  ) : null;

  const content = (
    <section
      className="action-dock"
      data-testid="action-dock"
      style={dockStyle}
    >
      {visibleActions.length > 0 ? (
        <div className="action-dock__response-stack">
          {claimCandidates.length > 0 ? (
            <div className="action-dock__claim-candidates" aria-label="可选吃碰杠组合">
              {claimCandidates.map((candidate, index) => (
                <button
                  key={candidate.key}
                  type="button"
                  className={`action-dock__claim-candidate action-dock__claim-candidate--${candidate.actionId} ${
                    candidate.isSelected ? 'action-dock__claim-candidate--selected' : ''
                  }`.trim()}
                  aria-label={`${candidate.actionLabel}候选组合 ${index + 1}`}
                  aria-pressed={candidate.isSelected}
                  onClick={() => onClaimCandidateSelect(candidate.actionId, candidate.tileIds)}
                  onDoubleClick={() => onClaimCandidateActivate(candidate.actionId, candidate.tileIds)}
                >
                  <span className="action-dock__claim-candidate-badge">{candidate.actionLabel}</span>
                  <span className="action-dock__claim-candidate-strip">
                    {candidate.tiles.map((tile, tileIndex) => (
                      <MahjongTile
                        key={`${candidate.key}-${tile.source}-${tile.code}-${tileIndex}`}
                        code={tile.code}
                        variant="discard"
                        relatedTileCode={selectedTileCode}
                        className={`action-dock__claim-preview-tile ${
                          tile.source === 'claim' ? 'action-dock__claim-preview-tile--claim' : ''
                        }`.trim()}
                      />
                    ))}
                  </span>
                </button>
              ))}
            </div>
          ) : null}
          <div className="action-dock__actions" aria-label="即时操作按钮">
            {visibleActions.map((action) => {
              const isPassAction = action.id === 'pass';
              const isHuAction = action.id === 'hu';
              const actionEffectClass = getActionEffectClass(action.id);

              return (
                <button
                  key={action.id}
                  type="button"
                  className={`action-dock__action action-dock__action--response ${
                    isPassAction ? 'action-dock__action--passive' : ''
                  } ${
                    isHuAction ? 'action-dock__action--hu-burn' : ''
                  } ${
                    actionEffectClass
                  }`.trim()}
                  onClick={() => onAction(action.id)}
                >
                  <span className="action-dock__action-label">{action.label}</span>
                </button>
              );
            })}
          </div>
        </div>
      ) : null}
      <div className="action-dock__tableau action-dock__tableau--full">
        <div className="action-dock__hand-zone">
          {isSpectator && onSwitchPerspective ? (
            <button
              type="button"
              className="action-dock__spectator-switch"
              aria-label={`切换观战视角，当前 ${spectatorFocusName ?? '未知玩家'}`}
              title="切换观战视角"
              onClick={onSwitchPerspective}
            >
              <span aria-hidden="true">↓</span>
            </button>
          ) : null}
          <div className="action-dock__hand-cluster">
            {hand.length > 0 ? (
              <div className="action-dock__hand" aria-label="Local hand">
                {hand.map((tile, index) => {
                  const isTileInteractionDisabled = tile.isDisabled || isSpectator || isHandInteractionDisabled;

                  return (
                    <button
                      key={`${tile.tileId}-${index}`}
                      type="button"
                      className={[
                        'action-dock__tile',
                        tile.isSelected ? 'action-dock__tile--selected' : '',
                        tile.isDrawn ? 'action-dock__tile--drawn' : '',
                        tile.isReplacementDrawn ? 'action-dock__tile--replacement-drawn' : '',
                        isTileInteractionDisabled ? 'action-dock__tile--disabled' : '',
                      ]
                        .filter(Boolean)
                        .join(' ')}
                      disabled={isTileInteractionDisabled}
                      aria-label={
                        isSpectator
                          ? `${tile.code} 观战模式`
                          : isHandInteractionDisabled
                            ? `${tile.code} BOT代打中`
                            : tile.isDisabled
                              ? `${tile.code} 当前回合禁止打出`
                              : undefined
                      }
                      onClick={(event) => {
                        if (isTileInteractionDisabled || event.detail > 1) {
                          return;
                        }

                        onTileSelect(tile.tileId);
                      }}
                      onDoubleClick={() => {
                        if (!isTileInteractionDisabled) {
                          onTileDoubleClick(tile.tileId);
                        }
                      }}
                    >
                      <MahjongTile
                        code={tile.code}
                        variant="hand"
                        isSelected={tile.isSelected}
                        isDrawn={tile.isDrawn}
                        isDisabled={isTileInteractionDisabled}
                        relatedTileCode={selectedTileCode}
                      />
                    </button>
                  );
                })}
              </div>
            ) : (
              <div className="action-dock__empty">牌桌进入对局后，手牌和操作按钮会显示在这里。</div>
            )}
            <div className="action-dock__info-rail">
              {handInsightControl ? (
                <div className="action-dock__status-group">
                  <div className="action-dock__status-group-standalone">{handInsightControl}</div>
                </div>
              ) : null}
            </div>
          </div>
        </div>
      </div>
    </section>
  );

  return content;
}

function HandInsightWinningFanItem({
  item,
  isPinned,
}: {
  item: NonNullable<BottomActionDockProps['handInsight']>['winningFans'][number];
  isPinned: boolean;
}) {
  const [isHovered, setIsHovered] = useState(false);
  const entry = isPinned ? getFanGuideEntry(item.fanKey) : null;

  return (
    <div
      className="action-dock__hand-insight-winning-fan"
      role="listitem"
      onMouseEnter={() => setIsHovered(true)}
      onMouseLeave={() => setIsHovered(false)}
    >
      <span>{getFanLabel(item.fanKey)}</span>
      <strong>{item.fanValue}番</strong>

      {isPinned && isHovered && entry && (
        <div className="action-dock__fan-detail-popover">
          <div className="action-dock__fan-detail-header">
            <strong>{entry.label}</strong>
            <span>{entry.fanValue}番</span>
          </div>
          <p>{entry.intro}</p>
          {entry.example && (
            <div className="action-dock__fan-detail-example">
              <small>例：</small>
              {entry.example}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

const ACTIVE_HAND_LAYOUT_COUNT = 14;
const WAITING_HAND_PLACEHOLDER_COUNT = 13;

const ACTION_PRIORITY: Partial<Record<BattleActionView['id'], number>> = {
  hu: 0,
  kong: 1,
  pung: 2,
  chow: 3,
  flower: 4,
  discard: 5,
  ready_hand: 6,
  pass: 7,
};

function getHandInsightTriggerLabel(handInsight: NonNullable<BottomActionDockProps['handInsight']>) {
  if (handInsight.source === 'selected_discard') {
    return '查看打出当前选中牌后的手牌洞察';
  }
  if (!handInsight.isTenpai && handInsight.winningFans.length > 0) {
    return '查看当前和牌番型';
  }
  return '查看当前听牌信息与和牌番型';
}

function getHandInsightPopoverLabel(handInsight: NonNullable<BottomActionDockProps['handInsight']>) {
  return handInsight.source === 'selected_discard' ? '打出后手牌洞察' : '当前手牌洞察';
}

function measureNaturalPopoverHeight(popover: HTMLElement) {
  const clone = popover.cloneNode(true) as HTMLElement;
  clone.classList.remove('action-dock__ready-hand-popover--horizontal');
  clone.setAttribute('aria-hidden', 'true');
  clone.style.position = 'fixed';
  clone.style.left = '-10000px';
  clone.style.top = '0';
  clone.style.visibility = 'hidden';
  clone.style.pointerEvents = 'none';
  clone.style.maxHeight = 'none';
  clone.style.overflow = 'visible';
  clone.style.animation = 'none';

  document.body.appendChild(clone);
  const rect = clone.getBoundingClientRect();
  const height = Math.max(rect.height, clone.scrollHeight);
  clone.remove();

  return height;
}

function getActionEffectClass(actionId: BattleActionView['id']) {
  const lookup: Partial<Record<BattleActionView['id'], string>> = {
    flower: 'action-dock__action--flower-bloom',
    chow: 'action-dock__action--themed action-dock__action--themed-chow',
    pung: 'action-dock__action--themed action-dock__action--themed-pung',
    kong: 'action-dock__action--themed action-dock__action--themed-kong',
    discard: 'action-dock__action--themed action-dock__action--themed-discard',
    ready_hand: 'action-dock__action--themed action-dock__action--themed-ready-hand',
    pass: 'action-dock__action--themed action-dock__action--themed-pass',
  };

  return lookup[actionId] ?? '';
}

function getDisplayedWinningFans(handInsight: NonNullable<BottomActionDockProps['handInsight']>) {
  return handInsight.winningFans
    .map((item, index) => ({ item, index }))
    .sort((left, right) => right.item.fanValue - left.item.fanValue || left.index - right.index)
    .map(({ item }) => item);
}
