import type {
  PlayerPresenceMessage,
  SeatSnapshot,
  UserPointsUpdatedMessage,
} from '../types/match';
import { TITLE_BANDS, titleForPoints, titleRank } from './titleBands';

export { titleDescriptionForTitle, titleForPoints } from './titleBands';

const NEUTRAL_TITLES = new Set(['熟练的码牌工', '弹性拆牌艺术家', '牌池人体扫描仪']);
const LOW_TITLE_BOUNDARY = titleRank('熟练的码牌工');

type BroadcastTone = 'low' | 'neutral' | 'high';

type PlayerBroadcastProfile = {
  name: string;
  title: string;
  tone: BroadcastTone;
};

function getTitleRank(title: string): number {
  return titleRank(title);
}

function getTitleTone(title: string): BroadcastTone {
  if (NEUTRAL_TITLES.has(title)) {
    return 'neutral';
  }

  return getTitleRank(title) < LOW_TITLE_BOUNDARY ? 'low' : 'high';
}

function getBroadcastPrefix(emoji: string): string {
  return `${emoji}系统播报：`;
}

function getSeat(seat: unknown, seats: SeatSnapshot[] | undefined): SeatSnapshot | null {
  return typeof seat === 'number'
    ? seats?.find((item) => item.seat_index === seat) ?? null
    : null;
}

function getPlayerProfile(
  seat: unknown,
  seats: SeatSnapshot[] | undefined,
  fallback?: Partial<Pick<SeatSnapshot, 'nickname' | 'points' | 'title'>>,
): PlayerBroadcastProfile {
  const seatSnapshot = getSeat(seat, seats);
  const name = fallback?.nickname ?? seatSnapshot?.nickname ?? (typeof seat === 'number' ? `玩家${seat + 1}` : '有人');
  const title =
    fallback?.title ??
    seatSnapshot?.title ??
    (typeof fallback?.points === 'number'
      ? titleForPoints(fallback.points)
        : typeof seatSnapshot?.points === 'number'
          ? titleForPoints(seatSnapshot.points)
          : TITLE_BANDS[LOW_TITLE_BOUNDARY].title);

  return {
    name,
    title,
    tone: getTitleTone(title),
  };
}

function formatPlayer(profile: PlayerBroadcastProfile): string {
  return `${profile.name}（${profile.title}）`;
}

function compose(emoji: string, copy: string): string {
  return `${getBroadcastPrefix(emoji)}${copy}`;
}

export function createPresenceSystemBroadcast(
  _payload: PlayerPresenceMessage['payload'],
  _seats?: SeatSnapshot[],
): string | null {
  return null;
}

export function createRoundEventSystemBroadcast(
  _eventType: string,
  _event: Record<string, unknown> = {},
  _seats?: SeatSnapshot[],
): string | null {
  return null;
}

export function createTitleChangeSystemBroadcast(payload: UserPointsUpdatedMessage['payload']): string | null {
  const oldTitle = payload.old_title ?? (typeof payload.old_points === 'number' ? titleForPoints(payload.old_points) : null);
  const newTitle = payload.title ?? titleForPoints(payload.points);
  if (!oldTitle || oldTitle === newTitle) {
    return null;
  }

  const name = payload.display_name ?? `用户 #${payload.user_id}`;
  const player = getPlayerProfile(null, undefined, {
    nickname: name,
    points: payload.points,
    title: newTitle,
  });
  const label = formatPlayer(player);
  const oldRank = getTitleRank(oldTitle);
  const newRank = getTitleRank(newTitle);

  if (player.tone === 'low') {
    const verb = newRank > oldRank ? '爬到' : '滑落到';
    return compose('🫠', `${label}从“${oldTitle}”${verb}“${newTitle}”，牌桌评价系统已经忍不住叹气。`);
  }

  if (player.tone === 'high') {
    const verb = newRank >= oldRank ? '登临' : '暂别';
    return compose('🏆', `${label}由“${oldTitle}”${verb}“${newTitle}”，牌桌礼仪进入仰望模式！`);
  }

  const verb = newRank > oldRank ? '晋升' : '调整';
  return compose('📣', `${label}成功由“${oldTitle}”${verb}“${newTitle}”！`);
}
