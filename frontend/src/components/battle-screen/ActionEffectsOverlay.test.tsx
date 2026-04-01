import { render } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { ActionEffectsOverlay } from './ActionEffectsOverlay';

describe('ActionEffectsOverlay', () => {
  it('does not render an action layer even when an action effect is provided', () => {
    render(
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

    expect(document.body.querySelector('.action-effects')).toBeNull();
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

    expect(document.body.querySelector('.action-effects')).toBeNull();
  });

  it('does not render a draw fallback effect when only drawnTileId changes', () => {
    render(
      <ActionEffectsOverlay
        actionEffect={null}
        celebrationEffect={null}
        drawnTileId="w2#draw-1"
      />,
    );

    expect(document.body.querySelector('.action-effects')).toBeNull();
  });
});
