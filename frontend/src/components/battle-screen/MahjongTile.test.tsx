import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { MahjongTile } from './MahjongTile';

describe('MahjongTile', () => {
  it('renders a suited tile face as an image instead of generated text', () => {
    render(<MahjongTile code="w1" variant="hand" />);

    const tile = screen.getByTestId('mahjong-tile');
    const image = screen.getByRole('img', { name: '一万牌面' });

    expect(tile).toHaveClass('mahjong-tile--retro');
    expect(tile).toHaveAttribute('aria-label', '一万');
    expect(image).toHaveAttribute('data-asset-name', '0101一萬.svg');
    expect(tile.querySelector('.mahjong-tile__face-image')).not.toBeNull();
    expect(screen.queryByText('万')).not.toBeInTheDocument();
    expect(screen.queryByText('一')).not.toBeInTheDocument();
  });

  it('renders sou tiles with the sou svg asset', () => {
    render(<MahjongTile code="c7" variant="discard" />);

    const tile = screen.getByTestId('mahjong-tile');
    expect(screen.getByRole('img', { name: '七条牌面' })).toHaveAttribute('data-asset-name', '0307七條.svg');
    expect(tile.querySelector('.mahjong-tile__face-viewport')).not.toBeNull();
  });

  it('renders flower tiles with the uploaded flower-face svg assets', () => {
    render(<MahjongTile code="f6" variant="discard" />);

    expect(screen.getByRole('img', { name: '兰牌面' })).toHaveAttribute('data-asset-name', '0506蘭.svg');
  });

  it('renders white dragon with the dedicated white-tile svg asset', () => {
    render(<MahjongTile code="white" variant="discard" />);

    const tile = screen.getByTestId('mahjong-tile');
    const image = screen.getByRole('img', { name: '白板牌面' });

    expect(tile.querySelector('.mahjong-tile__face-image')).not.toBeNull();
    expect(image).toHaveAttribute('data-asset-name', '0407白.svg');
  });

  it('marks the last discard variant with a dedicated state class', () => {
    render(<MahjongTile code="east" variant="discard" isLastDiscard />);

    expect(screen.getByTestId('mahjong-tile')).toHaveClass('mahjong-tile--last-discard');
  });

  it('marks freshly drawn hand tiles with the drawn state class', () => {
    render(<MahjongTile code="b4" variant="hand" isSelected isDrawn />);

    expect(screen.getByTestId('mahjong-tile')).toHaveClass('mahjong-tile--selected', 'mahjong-tile--drawn');
  });
});
