import { getTileAsset } from '../../lib/tileAssets';
import { formatTileName } from '../../lib/tileNames';

interface MahjongTileProps {
  code: string;
  variant: 'hand' | 'discard';
  isSelected?: boolean;
  isDrawn?: boolean;
  isLastDiscard?: boolean;
  className?: string;
}

export function MahjongTile({
  code,
  variant,
  isSelected = false,
  isDrawn = false,
  isLastDiscard = false,
  className,
}: MahjongTileProps) {
  const asset = getTileAsset(code);
  const tileName = formatTileName(code, code);
  const classes = [
    'mahjong-tile',
    'mahjong-tile--retro',
    `mahjong-tile--${variant}`,
    isSelected ? 'mahjong-tile--selected' : '',
    isDrawn ? 'mahjong-tile--drawn' : '',
    isLastDiscard ? 'mahjong-tile--last-discard' : '',
    className ?? '',
  ]
    .filter(Boolean)
    .join(' ');

  return (
    <span className={classes} data-testid="mahjong-tile" aria-label={`${tileName}`}>
      <span className="mahjong-tile__shell">
        <span className="mahjong-tile__face-area">
          <span className="mahjong-tile__face-viewport">
            {asset.kind === 'image' ? (
              <img
                className="mahjong-tile__face-image"
                src={asset.src}
                alt={`${tileName}牌面`}
                data-asset-name={asset.assetName}
              />
            ) : asset.kind === 'blank' ? (
              <span className="mahjong-tile__face-blank" aria-hidden="true" />
            ) : (
              <span className="mahjong-tile__face-placeholder" aria-hidden="true" />
            )}
          </span>
        </span>
      </span>
    </span>
  );
}
