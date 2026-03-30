import type { PlayerView } from '../../types/match';

interface PlayerRingProps {
  player: PlayerView;
}

export function PlayerRing({ player }: PlayerRingProps) {
  const windLabel = WIND_LABELS[player.wind] ?? player.wind;
  const presenceLabel = player.isBotControlled ? '离线' : player.connected ? '在线' : '离线';

  return (
    <section className={`player-slot player-slot--${player.seat}`}>
      <article
        className={`player-ring player-ring--${player.seat} ${player.isActive ? 'player-ring--active' : ''} ${
          player.isLocal ? 'player-ring--local' : ''
        }`}
      >
        <div className="player-ring__content">
          <div className="player-ring__row player-ring__row--primary">
            <strong>{player.name}</strong>
            <span className="player-ring__eyebrow">
              {windLabel}
              {player.isDealer ? ' 庄家' : ''}
              {` ${presenceLabel}`}
            </span>
          </div>
          <div className="player-ring__row player-ring__row--secondary">
            <span className="player-ring__meta">
              {player.score.toLocaleString()} · {player.statusText ?? '待命'}
            </span>
            <span className="player-ring__detail">
              手牌 {player.concealedCount} · 花 {player.flowerCount}
            </span>
          </div>
        </div>
      </article>
    </section>
  );
}

const WIND_LABELS: Record<PlayerView['wind'], string> = {
  East: '东',
  South: '南',
  West: '西',
  North: '北',
};
