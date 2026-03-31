import { act, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ActionEffectsOverlay } from './ActionEffectsOverlay';

describe('ActionEffectsOverlay', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.runOnlyPendingTimers();
    vi.useRealTimers();
  });

  it('does not get stuck visible when rerendered with the same action-effect key', () => {
    const { rerender } = render(
      <ActionEffectsOverlay
        actionEffect={{
          key: 'discard-1',
          label: '出牌',
          emphasis: 'discard',
          seat: 'left',
        }}
        celebrationEffect={null}
        drawnTileId={null}
      />,
    );

    expect(document.body.querySelector('.action-effects--action')).not.toBeNull();
    expect(document.body.querySelector('.action-effects__ring')).toBeNull();
    expect(document.body.querySelector('.action-effects__seal')).toBeNull();
    expect(document.body.querySelector('.action-effects__caption')).toBeNull();

    rerender(
      <ActionEffectsOverlay
        actionEffect={{
          key: 'discard-1',
          label: '出牌',
          emphasis: 'discard',
          seat: 'left',
        }}
        celebrationEffect={null}
        drawnTileId={null}
      />,
    );

    act(() => {
      vi.advanceTimersByTime(1700);
    });

    expect(document.body.querySelector('.action-effects--action')).toBeNull();
  });

  it('does not render the celebration layer even when a celebration effect is provided', () => {
    render(
      <ActionEffectsOverlay
        actionEffect={null}
        celebrationEffect={{
          key: 'win-1',
          label: '自摸',
          winnerSeat: 'bottom',
          winType: 'self_draw',
        }}
        drawnTileId={null}
      />,
    );

    expect(document.body.querySelector('.action-effects--celebration')).toBeNull();
  });
});
