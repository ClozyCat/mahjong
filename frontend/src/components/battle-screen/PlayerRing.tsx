import type { PlayerView } from '../../types/match';

interface PlayerRingProps {
  player: PlayerView;
}

export function PlayerRing({ player }: PlayerRingProps) {
  const windLabel = WIND_LABELS[player.wind] ?? player.wind;
  const deltaLabel = player.liveDelta === 0 ? '本局 0' : `本局 ${player.liveDelta > 0 ? '+' : ''}${player.liveDelta}`;

  return (
    <section className={`player-slot player-slot--${player.seat}`}>
      <article
        className={`player-ring player-ring--${player.seat} ${player.isActive ? 'player-ring--active' : ''} ${
          player.isLocal ? 'player-ring--local' : ''
        }`}
      >
        <div className="player-ring__avatar" aria-hidden="true" />
        <div className="player-ring__content">
          <span className="player-ring__eyebrow">
            {windLabel}
            {player.isDealer ? ' 庄家' : ''}
            {player.connected ? ' 在线' : ' 离线'}
          </span>
          <strong>{player.name}</strong>
          <span className="player-ring__meta">
            {player.score.toLocaleString()} · {player.statusText ?? '待命'}
          </span>
          <span className="player-ring__detail">
            准备 {player.ready ? '是' : '否'} · 手牌 {player.concealedCount} · 花 {player.flowerCount}
          </span>
          <span className="player-ring__detail">
            {deltaLabel}
          </span>
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
