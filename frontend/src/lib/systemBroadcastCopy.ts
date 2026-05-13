import type {
  PlayerPresenceMessage,
  SeatSnapshot,
  UserPointsUpdatedMessage,
} from '../types/match';
import { titleForPoints, titleRank } from './titleBands';

export { titleDescriptionForTitle, titleForPoints } from './titleBands';

function getTitleRank(title: string): number {
  return titleRank(title);
}

function getSeat(seat: unknown, seats: SeatSnapshot[] | undefined): SeatSnapshot | null {
  return typeof seat === 'number'
    ? seats?.find((item) => item.seat_index === seat) ?? null
    : null;
}

function getPlayerName(
  seat: unknown,
  seats: SeatSnapshot[] | undefined,
  fallback?: Partial<Pick<SeatSnapshot, 'nickname'>>,
): string {
  const seatSnapshot = getSeat(seat, seats);
  return fallback?.nickname ?? seatSnapshot?.nickname ?? (typeof seat === 'number' ? `玩家${seat + 1}` : '有人');
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
  const playerName = getPlayerName(null, undefined, {
    nickname: name,
  });
  const oldRank = getTitleRank(oldTitle);
  const newRank = getTitleRank(newTitle);

  return newRank >= oldRank
    ? `🎉${playerName}已由“${oldTitle}”飞升为“${newTitle}”🍾`
    : `👇${playerName}已由“${oldTitle}”陨落为“${newTitle}”💩`;
}
