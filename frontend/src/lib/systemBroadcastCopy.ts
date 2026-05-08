import type {
  PlayerPresenceMessage,
  SeatSnapshot,
  UserPointsUpdatedMessage,
} from '../types/match';

const NEUTRAL_TITLES = new Set(['大漏勺', '正分守门员', '概率论博导']);
const TITLE_ORDER = [
  '感动中国大善人',
  '赛博 ATM',
  '大漏勺',
  '正分守门员',
  '概率论博导',
  '大罗金仙',
  '只手遮天大魔王',
  '太上无极宇宙雀神',
] as const;

type BroadcastTone = 'low' | 'neutral' | 'high';

type PlayerBroadcastProfile = {
  name: string;
  title: string;
  tone: BroadcastTone;
};

export function titleForPoints(points: number): string {
  if (points <= -600) {
    return '感动中国大善人';
  }

  if (points <= 0) {
    return '赛博 ATM';
  }

  if (points <= 400) {
    return '大漏勺';
  }

  if (points <= 600) {
    return '正分守门员';
  }

  if (points <= 800) {
    return '概率论博导';
  }

  if (points <= 1200) {
    return '大罗金仙';
  }

  if (points <= 1800) {
    return '只手遮天大魔王';
  }

  return '太上无极宇宙雀神';
}

function getTitleRank(title: string): number {
  const index = TITLE_ORDER.indexOf(title as (typeof TITLE_ORDER)[number]);
  return index >= 0 ? index : TITLE_ORDER.indexOf('正分守门员');
}

function getTitleTone(title: string): BroadcastTone {
  if (NEUTRAL_TITLES.has(title)) {
    return 'neutral';
  }

  return getTitleRank(title) < TITLE_ORDER.indexOf('大漏勺') ? 'low' : 'high';
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
        : '正分守门员');

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

function strongestTone(profiles: PlayerBroadcastProfile[]): BroadcastTone {
  if (profiles.some((profile) => profile.tone === 'high')) {
    return 'high';
  }

  if (profiles.some((profile) => profile.tone === 'low')) {
    return 'low';
  }

  return 'neutral';
}

export function createPresenceSystemBroadcast(
  payload: PlayerPresenceMessage['payload'],
  seats?: SeatSnapshot[],
): string | null {
  if (!payload.connected) {
    return null;
  }

  const player = getPlayerProfile(payload.seat_index, seats, {
    nickname: payload.nickname ?? undefined,
    points: payload.points ?? undefined,
    title: payload.title ?? undefined,
  });
  const label = formatPlayer(player);

  if (player.tone === 'low') {
    return compose('🫠', `${label}摸进牌桌，分数先放一边，节目效果已经到位。`);
  }

  if (player.tone === 'high') {
    return compose('🌟', `${label}降临牌桌，众人先把计算器摆正。`);
  }

  return compose('🪑', `${label}进入牌局。`);
}

export function createRoundEventSystemBroadcast(
  eventType: string,
  event: Record<string, unknown> = {},
  seats?: SeatSnapshot[],
): string | null {
  if (eventType === 'ready_hand_declared') {
    const player = getPlayerProfile(event.seat, seats);
    const label = formatPlayer(player);

    if (player.tone === 'low') {
      return compose('🙃', `${label}居然听牌了，牌桌先暂停一下质疑。`);
    }

    if (player.tone === 'high') {
      return compose('🔮', `${label}进入听牌状态，牌局气压开始上升。`);
    }

    return compose('🀄', `${label}宣布听牌。`);
  }

  if (eventType === 'self_hu_declared') {
    const player = getPlayerProfile(event.seat, seats);
    const label = formatPlayer(player);

    if (player.tone === 'low') {
      return compose('🎯', `${label}自摸成功，这把算是把漏风口堵住了！`);
    }

    if (player.tone === 'high') {
      return compose('✨', `${label}自摸成功，牌桌向高分秩序低头！`);
    }

    return compose('🎉', `${label}自摸成功！`);
  }

  if (eventType === 'claim_made' && event.claim_type === 'hu') {
    const winner = getPlayerProfile(event.seat, seats);
    const discarder = getPlayerProfile(event.from, seats);
    const winnerLabel = formatPlayer(winner);
    const discarderLabel = formatPlayer(discarder);
    const tone = strongestTone([winner, discarder]);

    if (tone === 'low') {
      return compose('💥', `${discarderLabel}给${winnerLabel}放铳，这张牌打得很有反面教材价值。`);
    }

    if (tone === 'high') {
      return compose('⚡', `${winnerLabel}收下${discarderLabel}送上的铳牌，统治力继续加码！`);
    }

    return compose('🧨', `${discarderLabel}给${winnerLabel}放铳，${winner.name}荣和成功！`);
  }

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
