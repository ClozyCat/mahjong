import type { CSSProperties } from 'react';
import { useLayoutEffect, useRef, useState } from 'react';

import type { Seat } from '../../types/match';
import { MahjongTile } from './MahjongTile';

interface MeldRackProps {
  seat: Seat | 'local';
  melds: string[][];
  ariaLabel: string;
  emptyLabel?: string | null;
  collapsible?: boolean;
}

export function MeldRack({ seat, melds, ariaLabel, emptyLabel = null, collapsible = false }: MeldRackProps) {
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
                {meld.map((tile, tileIndex) => (
                  <MahjongTile
                    key={`${seat}-meld-${meldIndex}-${tile}-${tileIndex}`}
                    code={tile}
                    variant="discard"
                  />
                ))}
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
