import type { SeatSnapshot } from '../types/match';
import { formatTileName } from './tileNames';

function getSeatName(seat: unknown, seats: SeatSnapshot[] | undefined): string {
  if (typeof seat !== 'number') {
    return '有人';
  }

  return seats?.find((item) => item.seat_index === seat)?.nickname ?? `玩家${seat + 1}`;
}

function getTileCodeFromTileId(tileId: unknown): string | null {
  if (typeof tileId !== 'string') {
    return null;
  }

  const [tileCode] = tileId.split('#');
  return tileCode?.trim().toLowerCase() || null;
}

function getTileName(tileCode: string | null): string {
  return formatTileName(tileCode, '一张牌');
}

export function getRoundEventCopy(
  eventType: string,
  event: Record<string, unknown> = {},
  seats?: SeatSnapshot[],
): string {
  if (eventType === 'tile_drawn') {
    return `${getSeatName(event.seat, seats)}摸了一张牌`;
  }

  if (eventType === 'flower_exposed') {
    return `${getSeatName(event.seat, seats)}补花${getTileName(getTileCodeFromTileId(event.tile_id))}`;
  }

  if (eventType === 'replacement_draw') {
    return `${getSeatName(event.seat, seats)}完成补牌`;
  }

  if (eventType === 'tile_discarded') {
    const seatName = getSeatName(event.seat, seats);
    const tileName = getTileName(getTileCodeFromTileId(event.tile_id));
    return `${seatName}打出${tileName}`;
  }

  if (eventType === 'claim_made') {
    const claimType = CLAIM_TYPE_NAMES[String(event.claim_type)] ?? '响应';
    const tileName = getTileName(getTileCodeFromTileId(event.tile_id));
    return `${getSeatName(event.seat, seats)}${claimType}${tileName}`;
  }

  if (eventType === 'self_hu_declared') {
    return `${getSeatName(event.seat, seats)}已点和`;
  }

  if (eventType === 'self_kong_declared') {
    const kongType = KONG_TYPE_NAMES[String(event.kong_type)] ?? '杠';
    return `${getSeatName(event.seat, seats)}${kongType}${getTileName(String(event.tile_key ?? ''))}`;
  }

  if (eventType === 'claim_auto_passed') {
    return `吃碰杠胡响应超时，系统已自动过牌`;
  }

  if (eventType === 'rob_kong_auto_passed') {
    return `抢杠响应超时，系统已自动过牌`;
  }

  if (eventType === 'settlement_ready') {
    return '本局已进入结算';
  }

  if (eventType === 'round_drawn') {
    return '本局流局';
  }

  return '牌局状态已更新';
}

const CLAIM_TYPE_NAMES: Record<string, string> = {
  chow: '吃',
  pung: '碰',
  kong: '明杠',
  hu: '胡',
};

const KONG_TYPE_NAMES: Record<string, string> = {
  concealed_kong: '暗杠',
  add_kong: '补杠',
};
