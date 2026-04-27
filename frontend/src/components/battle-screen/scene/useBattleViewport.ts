import { useEffect, useLayoutEffect, useMemo, useState, type RefObject } from 'react';

import { isMobileDevice } from '../../../lib/device';

import { resolveTableLayoutProfile } from './layoutProfiles';
import type { BattleViewportMetrics, TableFxMode } from './types';

type NavigatorWithDeviceMemory = Navigator & {
  deviceMemory?: number;
};

function resolveElementSize(element: HTMLElement | null) {
  if (!element) {
    return null;
  }

  const rect = element.getBoundingClientRect();
  if (rect.width > 0 && rect.height > 0) {
    return {
      width: rect.width,
      height: rect.height,
    };
  }

  return null;
}

function clampViewportDimension(value: number, fallback: number) {
  if (!Number.isFinite(value) || value <= 0) {
    return fallback;
  }

  return value;
}

function shouldUseLowFx(width: number, height: number) {
  if (typeof window === 'undefined' || typeof navigator === 'undefined') {
    return false;
  }

  const coarsePointer = window.matchMedia?.('(pointer: coarse)').matches ?? false;
  const lowCoreCount = (navigator.hardwareConcurrency ?? Number.POSITIVE_INFINITY) <= 4;
  const lowMemory = ((navigator as NavigatorWithDeviceMemory).deviceMemory ?? Number.POSITIVE_INFINITY) <= 4;
  const crampedViewport = Math.min(width, height) < 640;

  return coarsePointer || isMobileDevice() || lowCoreCount || lowMemory || crampedViewport;
}

export function useBattleViewport(containerRef: RefObject<HTMLElement | null>): BattleViewportMetrics {
  const [viewport, setViewport] = useState(() => {
    const width =
      typeof window === 'undefined' ? 1280 : clampViewportDimension(window.innerWidth, 1280);
    const height =
      typeof window === 'undefined' ? 720 : clampViewportDimension(window.innerHeight, 720);
    const effectMode = shouldUseLowFx(width, height) ? 'lowFx' : 'fullFx';

    return {
      width,
      height,
      effectMode,
    };
  });

  useLayoutEffect(() => {
    if (typeof window === 'undefined') {
      return undefined;
    }

    const updateViewport = () => {
      const elementSize = resolveElementSize(containerRef.current);
      const width = clampViewportDimension(elementSize?.width ?? window.innerWidth, 1280);
      const height = clampViewportDimension(elementSize?.height ?? window.innerHeight, 720);
      setViewport({
        width,
        height,
        effectMode: shouldUseLowFx(width, height) ? 'lowFx' : 'fullFx',
      });
    };

    updateViewport();

    const resizeObserver =
      typeof ResizeObserver !== 'undefined' && containerRef.current
        ? new ResizeObserver(updateViewport)
        : null;

    resizeObserver?.observe(containerRef.current as HTMLElement);

    window.addEventListener('resize', updateViewport);

    const coarsePointerQuery = window.matchMedia?.('(pointer: coarse)');
    coarsePointerQuery?.addEventListener?.('change', updateViewport);

    return () => {
      resizeObserver?.disconnect();
      window.removeEventListener('resize', updateViewport);
      coarsePointerQuery?.removeEventListener?.('change', updateViewport);
    };
  }, [containerRef]);

  useEffect(() => {
    if (typeof document === 'undefined') {
      return undefined;
    }

    document.documentElement.dataset.battleFx = viewport.effectMode === 'lowFx' ? 'low' : 'full';

    return () => {
      if (document.documentElement.dataset.battleFx === (viewport.effectMode === 'lowFx' ? 'low' : 'full')) {
        delete document.documentElement.dataset.battleFx;
      }
    };
  }, [viewport.effectMode]);

  return useMemo(
    () => ({
      containerRef,
      width: viewport.width,
      height: viewport.height,
      aspectRatio: viewport.height > 0 ? viewport.width / viewport.height : 1,
      effectMode: viewport.effectMode as TableFxMode,
      layoutProfile: resolveTableLayoutProfile(viewport.width, viewport.height),
    }),
    [containerRef, viewport],
  );
}
