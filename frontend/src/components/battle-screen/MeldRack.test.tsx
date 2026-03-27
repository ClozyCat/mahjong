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
});
