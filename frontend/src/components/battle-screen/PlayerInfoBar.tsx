import type { PlayerView } from '../../types/match';
import { MahjongTile } from './MahjongTile';

export type TableStagePlayer = Pick<PlayerView, 'seat' | 'name' | 'melds'> &
  Partial<Omit<PlayerView, 'seat' | 'name' | 'melds'>>;

interface PlayerInfoBarProps {
  player: TableStagePlayer;
  className?: string;
  showSkillTooltip?: boolean;
  tooltipPlacement?: 'top' | 'right' | 'bottom' | 'left';
}

export function PlayerInfoBar({
  player,
  className = '',
  showSkillTooltip = false,
  tooltipPlacement = 'bottom',
}: PlayerInfoBarProps) {
  const skill = player.skill;
  const windLabel = player.wind ? (WIND_LABELS[player.wind] ?? player.wind) : null;
  const presenceLabel =
    player.seatType === 'bot'
      ? 'BOT'
      : player.connected === false
          ? '离线'
          : '在线';
  const eyebrowText = [windLabel, player.isDealer ? '庄家' : null, presenceLabel].filter(Boolean).join(' · ');
  const metaText = [
    typeof player.score === 'number' ? player.score.toLocaleString() : null,
    player.statusText ?? '待命',
  ]
    .filter(Boolean)
    .join(' · ');
  const detailText = `手牌 ${typeof player.concealedCount === 'number' ? player.concealedCount : '--'} · 花 ${
    typeof player.flowerCount === 'number' ? player.flowerCount : '--'
  }`;

  return (
    <article
      className={`table-stage__player-info ${player.isActive ? 'table-stage__player-info--active' : ''} ${
        player.isLocal ? 'table-stage__player-info--local' : ''
      } ${className}`.trim()}
      aria-label={`${player.name} 信息栏`}
    >
      <span className="table-stage__player-info-eyebrow">{eyebrowText}</span>
      <strong className="table-stage__player-info-name">{player.name}</strong>
      <span className="table-stage__player-info-meta">{metaText}</span>
      <span className="table-stage__player-info-detail">{detailText}</span>
      {showSkillTooltip && skill ? (
        <div
          className={`table-stage__skill-tooltip table-stage__skill-tooltip--${skill.tone} table-stage__skill-tooltip--seat-${tooltipPlacement}`.trim()}
          role="tooltip"
        >
          <div className="table-stage__skill-tooltip-header">
            <span className="table-stage__skill-tooltip-rarity">{skill.rarityLabel}</span>
            <span className="table-stage__skill-tooltip-type">{skill.typeLabel}</span>
          </div>
          <strong className="table-stage__skill-tooltip-name">{skill.name}</strong>
          <p className="table-stage__skill-tooltip-summary">{skill.summary}</p>
          <p className="table-stage__skill-tooltip-detail">{skill.detail}</p>
          <div className="table-stage__skill-tooltip-meta">
            <span>剩余 {skill.remainingRounds} 局</span>
            {skill.type === 'active' ? <span>本局剩余 {skill.remainingActivationsThisRound} 次</span> : null}
          </div>
          {skill.previewTileKeys?.length ? (
            <div className="table-stage__skill-tooltip-preview">
              <span className="table-stage__skill-tooltip-preview-label">
                {skill.interactionKind === 'preview_wall' ? '已查看尾牌' : '已获得情报'}
              </span>
              <div className="table-stage__skill-tooltip-preview-tiles" aria-label={`${skill.name} 已查看牌`}>
                {skill.previewTileKeys.map((tileKey, index) => (
                  <MahjongTile
                    key={`${skill.skillId}-${tileKey}-${index}`}
                    code={tileKey}
                    variant="discard"
                    className="table-stage__skill-tooltip-preview-tile"
                  />
                ))}
              </div>
            </div>
          ) : null}
          {skill.interactionHint ? (
            <p className="table-stage__skill-tooltip-hint">{skill.interactionHint}</p>
          ) : null}
        </div>
      ) : null}
    </article>
  );
}

const WIND_LABELS: Partial<Record<PlayerView['wind'], string>> = {
  East: '东',
  South: '南',
  West: '西',
  North: '北',
};
