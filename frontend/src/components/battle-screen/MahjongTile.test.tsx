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
    expect(image).toHaveAttribute('data-asset-name', '1man.svg');
    expect(tile.querySelector('.mahjong-tile__face-image')).not.toBeNull();
    expect(screen.queryByText('万')).not.toBeInTheDocument();
    expect(screen.queryByText('一')).not.toBeInTheDocument();
  });

  it('renders sou tiles with the sou svg asset', () => {
    render(<MahjongTile code="c7" variant="discard" />);

    const tile = screen.getByTestId('mahjong-tile');
    expect(screen.getByRole('img', { name: '七条牌面' })).toHaveAttribute('data-asset-name', '7sou.svg');
    expect(tile.querySelector('.mahjong-tile__face-viewport')).not.toBeNull();
  });

  it('renders flower tiles with the uploaded flower-face svg assets', () => {
    render(<MahjongTile code="f6" variant="discard" />);

    expect(screen.getByRole('img', { name: '兰牌面' })).toHaveAttribute('data-asset-name', 'orchid.svg');
  });

  it('renders white dragon as a blank face without an image', () => {
    render(<MahjongTile code="white" variant="discard" />);

    const tile = screen.getByTestId('mahjong-tile');

    expect(tile.querySelector('.mahjong-tile__face-blank')).not.toBeNull();
    expect(screen.queryByRole('img')).not.toBeInTheDocument();
  });

  it('marks the last discard variant with a dedicated state class', () => {
    render(<MahjongTile code="east" variant="discard" isLastDiscard />);

    expect(screen.getByTestId('mahjong-tile')).toHaveClass('mahjong-tile--last-discard');
  });

  it('renders a drawn indicator for freshly drawn hand tiles', () => {
    render(<MahjongTile code="b4" variant="hand" isSelected isDrawn />);

    expect(screen.getByTestId('mahjong-tile')).toHaveClass('mahjong-tile--selected', 'mahjong-tile--drawn');
    expect(screen.getByTestId('mahjong-tile-drawn-indicator')).toBeInTheDocument();
  });
});
