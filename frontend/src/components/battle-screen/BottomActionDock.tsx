import type { CSSProperties } from 'react';
import { useEffect, useRef, useState } from 'react';

import type {
  BackendActionType,
  BattleActionView,
  BattlePromptView,
  BattleViewModel,
  ClaimActionId,
} from '../../types/match';
import { MahjongTile } from './MahjongTile';

interface BottomActionDockProps {
  hand: BattleViewModel['localHand'];
  readyHandInsight?: BattleViewModel['readyHandInsight'];
  claimCandidates: BattleViewModel['claimCandidates'];
  actions: BattleActionView[];
  isElevated: boolean;
  isWaitingForMatchStart?: boolean;
  promptCue: BattlePromptView | null;
  deadlineAt: string | null;
  onTileSelect: (tileId: string) => void;
  onTileDoubleClick: (tileId: string) => void;
  onClaimCandidateSelect: (actionId: ClaimActionId, tileIds: string[]) => void;
  onClaimCandidateActivate: (actionId: ClaimActionId, tileIds: string[]) => void;
  onAction: (actionId: BattleActionView['id']) => void;
}

export function BottomActionDock({
  hand,
  readyHandInsight = null,
  claimCandidates,
  actions,
  isElevated,
  isWaitingForMatchStart = false,
  promptCue,
  deadlineAt,
  onTileSelect,
  onTileDoubleClick,
  onClaimCandidateSelect,
  onClaimCandidateActivate,
  onAction,
}: BottomActionDockProps) {
  const [isReadyHandPopoverHovered, setIsReadyHandPopoverHovered] = useState(false);
  const [isReadyHandPopoverPinned, setIsReadyHandPopoverPinned] = useState(false);
  const readyHandPopoverRef = useRef<HTMLDivElement | null>(null);
  const handCount = hand.length;
  const hasDrawnTile = hand.some((tile) => tile.isDrawn || tile.isReplacementDrawn);
  const layoutHandCount = handCount > 0 ? handCount : isWaitingForMatchStart ? WAITING_HAND_PLACEHOLDER_COUNT : 1;
  const dockStyle = {
    '--action-dock-hand-count': `${handCount}`,
    '--action-dock-effective-hand-count': `${Math.max(handCount, 1)}`,
    '--action-dock-gap-count': `${Math.max(handCount - 1, 0)}`,
    '--action-dock-drawn-gap-count': hasDrawnTile ? '1' : '0',
    '--action-dock-layout-hand-count': `${layoutHandCount}`,
    '--action-dock-layout-gap-count': `${Math.max(layoutHandCount - 1, 0)}`,
  } as CSSProperties;
  const visibleActions = actions
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
    );
  const shouldElevateDock = isElevated && !isResponsePrompt(promptCue);
  const isReadyHandPopoverOpen =
    Boolean(readyHandInsight) && (isReadyHandPopoverHovered || isReadyHandPopoverPinned);

  useEffect(() => {
    if (!isReadyHandPopoverPinned) {
      return undefined;
    }

    function handlePointerDown(event: PointerEvent) {
      if (!readyHandPopoverRef.current?.contains(event.target as Node)) {
        setIsReadyHandPopoverPinned(false);
      }
    }

    window.addEventListener('pointerdown', handlePointerDown);
    return () => window.removeEventListener('pointerdown', handlePointerDown);
  }, [isReadyHandPopoverPinned]);

  useEffect(() => {
    if (readyHandInsight) {
      return;
    }

    setIsReadyHandPopoverHovered(false);
    setIsReadyHandPopoverPinned(false);
  }, [readyHandInsight]);

  const readyHandControl = readyHandInsight ? (
    <div
      ref={readyHandPopoverRef}
      className="action-dock__ready-hand-anchor"
      onMouseEnter={() => setIsReadyHandPopoverHovered(true)}
      onMouseLeave={() => setIsReadyHandPopoverHovered(false)}
    >
      <button
        type="button"
        className={`action-dock__ready-hand-trigger ${
          isReadyHandPopoverOpen ? 'action-dock__ready-hand-trigger--open' : ''
        }`.trim()}
        aria-label={getReadyHandTriggerLabel(readyHandInsight)}
        aria-expanded={isReadyHandPopoverOpen}
        onClick={() => setIsReadyHandPopoverPinned((currentValue) => !currentValue)}
      >
        i
      </button>
      {isReadyHandPopoverOpen ? (
        <section className="action-dock__ready-hand-popover" aria-label={getReadyHandPopoverLabel(readyHandInsight)}>
          <div className="action-dock__ready-hand-list" role="list">
            {readyHandInsight.waits.map((wait) => (
              <div key={wait.code} className="action-dock__ready-hand-row" role="listitem">
                <div className="action-dock__ready-hand-tile">
                  <MahjongTile code={wait.code} variant="discard" className="action-dock__ready-hand-preview-tile" />
                </div>
                <strong>{wait.availableCount}</strong>
              </div>
            ))}
          </div>
        </section>
      ) : null}
    </div>
  ) : null;

  const content = (
    <section
      className={`action-dock ${shouldElevateDock ? 'action-dock--elevated' : ''}`}
      data-testid="action-dock"
      data-elevated={shouldElevateDock}
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
          {hand.length > 0 ? (
            <div className="action-dock__hand" aria-label="Local hand">
              {hand.map((tile, index) => (
                <button
                  key={`${tile.tileId}-${index}`}
                  type="button"
                  className={[
                    'action-dock__tile',
                    tile.isSelected ? 'action-dock__tile--selected' : '',
                    tile.isDrawn ? 'action-dock__tile--drawn' : '',
                    tile.isReplacementDrawn ? 'action-dock__tile--replacement-drawn' : '',
                    tile.isDisabled ? 'action-dock__tile--disabled' : '',
                  ]
                    .filter(Boolean)
                    .join(' ')}
                  disabled={tile.isDisabled}
                  aria-label={tile.isDisabled ? `${tile.code} 当前回合禁止打出` : undefined}
                  onClick={(event) => {
                    if (event.detail > 1) {
                      return;
                    }

                    onTileSelect(tile.tileId);
                  }}
                  onDoubleClick={() => onTileDoubleClick(tile.tileId)}
                >
                  <MahjongTile
                    code={tile.code}
                    variant="hand"
                    isSelected={tile.isSelected}
                    isDrawn={tile.isDrawn}
                    isDisabled={tile.isDisabled}
                  />
                </button>
              ))}
            </div>
          ) : (
            <div className="action-dock__empty">牌桌进入对局后，手牌和操作按钮会显示在这里。</div>
          )}
          <div className="action-dock__info-rail">
            {readyHandControl ? (
              <div className="action-dock__status-group">
                <div className="action-dock__status-group-standalone">{readyHandControl}</div>
              </div>
            ) : null}
          </div>
        </div>
      </div>
    </section>
  );

  return content;
}

const WAITING_HAND_PLACEHOLDER_COUNT = 13;

const ACTION_PRIORITY: Partial<Record<BattleActionView['id'], number>> = {
  hu: 0,
  kong: 1,
  pung: 2,
  chow: 3,
  flower: 4,
  discard: 5,
  pass: 6,
};

function isResponsePrompt(promptCue: BattlePromptView | null) {
  return promptCue?.kind === 'claim' || promptCue?.kind === 'rob_kong' || promptCue?.kind === 'turn_kong';
}

function getReadyHandTriggerLabel(readyHandInsight: NonNullable<BottomActionDockProps['readyHandInsight']>) {
  return readyHandInsight.source === 'selected_discard'
    ? '查看打出当前选中牌后的听牌信息'
    : '查看当前听牌信息';
}

function getReadyHandPopoverLabel(readyHandInsight: NonNullable<BottomActionDockProps['readyHandInsight']>) {
  return readyHandInsight.source === 'selected_discard' ? '打出后听牌信息' : '当前听牌信息';
}

function getActionEffectClass(actionId: BattleActionView['id']) {
  const lookup: Partial<Record<BattleActionView['id'], string>> = {
    flower: 'action-dock__action--flower-bloom',
    chow: 'action-dock__action--themed action-dock__action--themed-chow',
    pung: 'action-dock__action--themed action-dock__action--themed-pung',
    kong: 'action-dock__action--themed action-dock__action--themed-kong',
    discard: 'action-dock__action--themed action-dock__action--themed-discard',
    pass: 'action-dock__action--themed action-dock__action--themed-pass',
  };

  return lookup[actionId] ?? '';
}
