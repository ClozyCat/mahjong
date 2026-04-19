import { fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { MeldRack } from './MeldRack';

let mockMeldRackScrollHeight = 108;

describe('MeldRack', () => {
  beforeEach(() => {
    mockMeldRackScrollHeight = 108;

    vi.spyOn(HTMLElement.prototype, 'offsetHeight', 'get').mockImplementation(function mockOffsetHeight(this: HTMLElement) {
      return this.classList.contains('meld-rack__group') ? 44 : 0;
    });

    vi.spyOn(HTMLElement.prototype, 'scrollHeight', 'get').mockImplementation(function mockScrollHeight(this: HTMLElement) {
      return this.classList.contains('meld-rack') ? mockMeldRackScrollHeight : 0;
    });
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('shows a compact expand control when remote melds overflow a single row', () => {
    render(
      <MeldRack
        seat="top"
        melds={[
          ['b1', 'b2', 'b3'],
          ['c3', 'c4', 'c5'],
          ['w7', 'w8', 'w9'],
        ]}
        ariaLabel="Opponent melds"
        collapsible
      />,
    );

    const shell = screen.getByLabelText('Opponent melds');
    const expandButton = screen.getByRole('button', { name: '展开 Opponent melds' });

    expect(shell.querySelector('.meld-rack--collapsed')).not.toBeNull();
    expect(expandButton).toBeInTheDocument();

    fireEvent.mouseEnter(expandButton);
    expect(shell).toHaveClass('meld-rack-shell--expanded');

    fireEvent.mouseLeave(shell);
    expect(shell).not.toHaveClass('meld-rack-shell--expanded');
  });

  it('keeps the rack fully open when melds fit in one row', () => {
    mockMeldRackScrollHeight = 44;

    render(
      <MeldRack
        seat="left"
        melds={[
          ['b1', 'b2', 'b3'],
          ['c3', 'c4', 'c5'],
        ]}
        ariaLabel="Compact melds"
        collapsible
      />,
    );

    const shell = screen.getByLabelText('Compact melds');

    expect(shell.querySelector('.meld-rack__toggle')).toBeNull();
    expect(shell.querySelector('.meld-rack--collapsed')).toBeNull();
  });

  it('marks sourced meld tiles with centered source indicators that encode triangle direction', () => {
    render(
      <MeldRack
        seat="right"
        melds={[
          {
            tiles: [
              { code: 'w1', orientation: 'rotated' },
              { code: 'w1', orientation: 'normal' },
              { code: 'w1', orientation: 'normal' },
            ],
          },
          {
            tiles: [
              { code: 'w2', orientation: 'normal' },
              { code: 'w2', orientation: 'rotated' },
              { code: 'w2', orientation: 'normal' },
            ],
          },
          {
            tiles: [
              { code: 'w3', orientation: 'normal' },
              { code: 'w3', orientation: 'normal' },
              { code: 'w3', orientation: 'normal' },
              { code: 'w3', orientation: 'rotated' },
            ],
          },
          {
            tiles: [
              { code: 'w4', orientation: 'normal' },
              { code: 'w4', orientation: 'normal' },
              { code: 'w4', orientation: 'upside_down' as const },
              { code: 'w4', orientation: 'face_down' },
            ],
          },
        ]}
        ariaLabel="Claim melds"
      />,
    );

    const rack = screen.getByLabelText('Claim melds');
    const sourceIndicators = rack.querySelectorAll<HTMLElement>('.meld-rack__source-indicator');

    expect(rack.querySelectorAll('.meld-rack__tile--sourced')).toHaveLength(4);
    expect(sourceIndicators).toHaveLength(4);
    expect(sourceIndicators[0]).toHaveAttribute('data-claim-source', 'left-player');
    expect(sourceIndicators[0]).toHaveAttribute('data-triangle-direction', 'point-left');
    expect(sourceIndicators[0].style.getPropertyValue('--meld-rack-triangle-angle')).toBe('-90deg');
    expect(sourceIndicators[1]).toHaveAttribute('data-claim-source', 'across-player');
    expect(sourceIndicators[1]).toHaveAttribute('data-triangle-direction', 'point-up');
    expect(sourceIndicators[1].style.getPropertyValue('--meld-rack-triangle-angle')).toBe('0deg');
    expect(sourceIndicators[2]).toHaveAttribute('data-claim-source', 'right-player');
    expect(sourceIndicators[2]).toHaveAttribute('data-triangle-direction', 'point-right');
    expect(sourceIndicators[2].style.getPropertyValue('--meld-rack-triangle-angle')).toBe('90deg');
    expect(sourceIndicators[3]).toHaveAttribute('data-claim-source', 'across-player');
    expect(sourceIndicators[3]).toHaveAttribute('data-triangle-direction', 'point-up');
    expect(sourceIndicators[3].style.getPropertyValue('--meld-rack-triangle-angle')).toBe('0deg');
    expect(rack.querySelector('.mahjong-tile__face-blank')).not.toBeNull();
  });

  it('keeps chow source markers pointing to the left player even when the claimed tile is the middle tile', () => {
    render(
      <MeldRack
        seat="right"
        melds={[
          {
            tiles: [
              { code: 'w2', orientation: 'normal' },
              { code: 'w3', orientation: 'rotated' },
              { code: 'w4', orientation: 'normal' },
            ],
          },
        ]}
        ariaLabel="Chow melds"
      />,
    );

    const sourceIndicator = screen
      .getByLabelText('Chow melds')
      .querySelector<HTMLElement>('.meld-rack__source-indicator');

    expect(sourceIndicator).toHaveAttribute('data-claim-source', 'left-player');
    expect(sourceIndicator).toHaveAttribute('data-triangle-direction', 'point-left');
    expect(sourceIndicator?.style.getPropertyValue('--meld-rack-triangle-angle')).toBe('-90deg');
  });
});
