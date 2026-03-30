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

    expect(screen.getAllByText('出牌').length).toBeGreaterThan(0);

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

    expect(screen.queryByText('出牌')).not.toBeInTheDocument();
  });
});
