import type { CSSProperties } from 'react';
import { useLayoutEffect, useRef, useState } from 'react';

import type { DisplayMeldView, PlayerMeldView, Seat } from '../../types/match';
import { MahjongTile } from './MahjongTile';

type MeldClaimSource = 'left-player' | 'across-player' | 'right-player';
type TriangleDirection = 'point-left' | 'point-up' | 'point-right';

interface MeldRackProps {
  seat: Seat | 'local';
  melds: PlayerMeldView[];
  ariaLabel: string;
  emptyLabel?: string | null;
  collapsible?: boolean;
  selectedTileCode?: string | null;
}

export function MeldRack({
  seat,
  melds,
  ariaLabel,
  emptyLabel = null,
  collapsible = false,
  selectedTileCode = null,
}: MeldRackProps) {
  const hasMelds = melds.length > 0;
  const rackRef = useRef<HTMLDivElement>(null);
  const [isExpanded, setIsExpanded] = useState(false);
  const [isOverflowing, setIsOverflowing] = useState(false);
  const [collapsedHeight, setCollapsedHeight] = useState<number | null>(null);

  useLayoutEffect(() => {
    const rackElement = rackRef.current;

    if (!rackElement || !hasMelds) {
      setIsOverflowing(false);
      setCollapsedHeight(null);
      setIsExpanded(false);
      return;
    }

    const measureRack = () => {
      const firstGroup = rackElement.querySelector<HTMLElement>('.meld-rack__group');
      const nextCollapsedHeight = firstGroup?.offsetHeight ?? 0;
      const nextOverflowing = collapsible && nextCollapsedHeight > 0 && rackElement.scrollHeight > nextCollapsedHeight + 1;

      setCollapsedHeight(nextCollapsedHeight || null);
      setIsOverflowing(nextOverflowing);

      if (!nextOverflowing) {
        setIsExpanded(false);
      }
    };

    measureRack();

    if (typeof ResizeObserver !== 'undefined') {
      const observer = new ResizeObserver(() => {
        measureRack();
      });

      observer.observe(rackElement);
      Array.from(rackElement.children).forEach((child) => observer.observe(child));

      return () => observer.disconnect();
    }

    window.addEventListener('resize', measureRack);

    return () => window.removeEventListener('resize', measureRack);
  }, [collapsible, hasMelds, melds]);

  const shouldShowToggle = hasMelds && isOverflowing;
  const rackStyle = shouldShowToggle && collapsedHeight
    ? ({ '--meld-rack-collapsed-height': `${collapsedHeight}px` } as CSSProperties)
    : undefined;

  return (
    <div
      className={`meld-rack-shell ${shouldShowToggle ? 'meld-rack-shell--collapsible' : ''} ${
        isExpanded ? 'meld-rack-shell--expanded' : ''
      }`}
      aria-label={ariaLabel}
      onMouseLeave={() => setIsExpanded(false)}
    >
      <div
        ref={rackRef}
        className={`meld-rack meld-rack--${seat} ${hasMelds ? '' : 'meld-rack--empty'} ${
          shouldShowToggle ? 'meld-rack--collapsed' : ''
        }`}
        style={rackStyle}
      >
        {hasMelds
          ? melds.map((meld, meldIndex) => (
              <div key={`${seat}-meld-${meldIndex}`} className="meld-rack__group">
                {normalizeMeldTiles(meld).map((tile, tileIndex, tiles) => {
                  const isSourcedTile = tile.orientation === 'rotated' || tile.orientation === 'upside_down';
                  const claimSource = getClaimSource(tile, tileIndex, tiles);
                  const triangleDirection = getTriangleDirection(claimSource);

                  return (
                    <span
                      key={`${seat}-meld-${meldIndex}-${tile.code}-${tileIndex}`}
                      className={`meld-rack__tile ${isSourcedTile ? 'meld-rack__tile--sourced' : ''}`.trim()}
                    >
                      {isSourcedTile && (
                        <span
                          className="meld-rack__source-indicator"
                          data-claim-source={claimSource}
                          data-triangle-direction={triangleDirection}
                          style={getSourceIndicatorStyle(triangleDirection)}
                          aria-hidden="true"
                        />
                      )}
                      <MahjongTile
                        code={tile.code}
                        variant="discard"
                        isFaceDown={tile.orientation === 'face_down'}
                        relatedTileCode={selectedTileCode}
                        className={
                          isSourcedTile
                            ? 'meld-rack__tile-face--sourced'
                            : undefined
                        }
                      />
                    </span>
                  );
                })}
              </div>
            ))
          : emptyLabel
            ? <span className="meld-rack__empty-text">{emptyLabel}</span>
            : null}
      </div>
      {shouldShowToggle ? (
        <button
          type="button"
          className="meld-rack__toggle"
          aria-label={`展开 ${ariaLabel}`}
          onMouseEnter={() => setIsExpanded(true)}
          onFocus={() => setIsExpanded(true)}
          onBlur={() => setIsExpanded(false)}
        >
          <span aria-hidden="true">▾</span>
        </button>
      ) : null}
    </div>
  );
}

function normalizeMeldTiles(meld: PlayerMeldView): DisplayMeldView['tiles'] {
  if (Array.isArray(meld)) {
    return meld.map((code) => ({ code, orientation: 'normal' as const }));
  }

  return meld.tiles;
}

function getClaimSource(
  tile: DisplayMeldView['tiles'][number],
  tileIndex: number,
  tiles: DisplayMeldView['tiles'],
): MeldClaimSource {
  if (tile.orientation === 'upside_down') {
    return 'across-player';
  }

  if (isChowTileRun(tiles)) {
    return 'left-player';
  }

  const tileCount = tiles.length;
  if (tileIndex <= 0) {
    return 'left-player';
  }

  if (tileIndex >= tileCount - 1) {
    return 'right-player';
  }

  return 'across-player';
}

function isChowTileRun(tiles: DisplayMeldView['tiles'] | undefined) {
  if (!tiles || tiles.length !== 3) {
    return false;
  }

  const parsedTiles = tiles.map((tile) => parseSuitedTile(tile.code));
  if (parsedTiles.some((tile) => tile === null)) {
    return false;
  }

  const [firstTile, ...restTiles] = parsedTiles as Array<{ suit: string; rank: number }>;
  if (restTiles.some((tile) => tile.suit !== firstTile.suit)) {
    return false;
  }

  const sortedRanks = parsedTiles
    .map((tile) => (tile as { suit: string; rank: number }).rank)
    .slice()
    .sort((left, right) => left - right);

  return sortedRanks[0] + 1 === sortedRanks[1] && sortedRanks[1] + 1 === sortedRanks[2];
}

function parseSuitedTile(code: string) {
  const match = /^([a-z])([1-9])$/i.exec(code.trim());
  if (!match) {
    return null;
  }

  const normalizedSuit = normalizeSuitedTileFamily(match[1].toLowerCase());
  if (!normalizedSuit) {
    return null;
  }

  return {
    suit: normalizedSuit,
    rank: Number(match[2]),
  };
}

function normalizeSuitedTileFamily(suit: string) {
  if (suit === 'w' || suit === 'm') {
    return 'wan';
  }

  if (suit === 'b' || suit === 'p') {
    return 'tong';
  }

  if (suit === 'c' || suit === 't') {
    return 'tiao';
  }

  return null;
}

function getTriangleDirection(source: MeldClaimSource): TriangleDirection {
  if (source === 'left-player') {
    return 'point-left';
  }

  if (source === 'right-player') {
    return 'point-right';
  }

  return 'point-up';
}

function getSourceIndicatorStyle(direction: TriangleDirection): CSSProperties {
  return {
    '--meld-rack-triangle-angle': getTriangleAngle(direction),
  } as CSSProperties;
}

function getTriangleAngle(direction: TriangleDirection) {
  if (direction === 'point-left') {
    return '-90deg';
  }

  if (direction === 'point-right') {
    return '90deg';
  }

  return '0deg';
}
