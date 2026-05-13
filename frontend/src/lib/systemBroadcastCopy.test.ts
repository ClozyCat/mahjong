import { describe, expect, it } from 'vitest';

import {
  createPresenceSystemBroadcast,
  createRoundEventSystemBroadcast,
  createTitleChangeSystemBroadcast,
  titleForPoints,
} from './systemBroadcastCopy';

const seats = [
  { seat_index: 0, user_id: 10, nickname: '小A', points: 550, title: '入门雀友', connected: true },
  { seat_index: 1, user_id: 11, nickname: '小B', points: -20, title: '散财童子', connected: true },
  { seat_index: 2, user_id: 12, nickname: '小C', points: 1350, title: '至尊雀神', connected: true },
];

describe('system broadcast copy', () => {
  it('maps player titles with 100-point lower-inclusive bands', () => {
    expect(titleForPoints(-1000)).toBe('散财童子');
    expect(titleForPoints(49)).toBe('散财童子');
    expect(titleForPoints(50)).toBe('点炮能手');
    expect(titleForPoints(550)).toBe('入门雀友');
    expect(titleForPoints(649)).toBe('入门雀友');
    expect(titleForPoints(650)).toBe('初露锋芒');
    expect(titleForPoints(9999)).toBe('至尊雀神');
  });

  it('does not create a ready-hand system broadcast', () => {
    const copy = createRoundEventSystemBroadcast('ready_hand_declared', { seat: 0 }, seats);

    expect(copy).toBeNull();
  });

  it('does not create a player-entry system broadcast', () => {
    const copy = createPresenceSystemBroadcast(
      { table_code: 'ROOM1', seat_index: 1, connected: true },
      seats,
    );

    expect(copy).toBeNull();
  });

  it('does not create a self-draw system broadcast', () => {
    const copy = createRoundEventSystemBroadcast('self_hu_declared', { seat: 2 }, seats);

    expect(copy).toBeNull();
  });

  it('does not create a discard-win system broadcast', () => {
    const copy = createRoundEventSystemBroadcast(
      'claim_made',
      { seat: 0, from: 1, claim_type: 'hu' },
      seats,
    );

    expect(copy).toBeNull();
  });

  it('announces title promotions without appending the new title to the player name', () => {
    const copy = createTitleChangeSystemBroadcast({
      user_id: 10,
      display_name: '小A',
      delta: 20,
      old_points: 540,
      points: 560,
      old_title: '迷途麻客',
      title: '入门雀友',
      reason: 'round_settlement',
      source_table_code: 'ROOM1',
      source_round_id: 'east-1',
    });

    expect(copy).toBe('🎉小A已由“迷途麻客”飞升为“入门雀友”🍾');
  });

  it('announces title demotions without appending the old title to the player name', () => {
    const copy = createTitleChangeSystemBroadcast({
      user_id: 11,
      display_name: '小B',
      delta: -20,
      old_points: 560,
      points: 540,
      old_title: '入门雀友',
      title: '迷途麻客',
      reason: 'round_settlement',
      source_table_code: 'ROOM1',
      source_round_id: 'east-1',
    });

    expect(copy).toBe('👇小B已由“入门雀友”陨落为“迷途麻客”💩');
  });

  it('only creates system broadcasts for title changes', () => {
    const entry = createPresenceSystemBroadcast(
      { table_code: 'ROOM1', seat_index: 2, connected: true },
      seats,
    );
    const readyHand = createRoundEventSystemBroadcast('ready_hand_declared', { seat: 2 }, seats);
    const selfDraw = createRoundEventSystemBroadcast('self_hu_declared', { seat: 2 }, seats);
    const discardWin = createRoundEventSystemBroadcast(
      'claim_made',
      { seat: 2, from: 0, claim_type: 'hu' },
      seats,
    );
    const titleChange = createTitleChangeSystemBroadcast({
      user_id: 12,
      display_name: '小C',
      delta: 700,
      old_points: 700,
      points: 1400,
      old_title: '初露锋芒',
      title: '至尊雀神',
      reason: 'round_settlement',
      source_table_code: 'ROOM1',
      source_round_id: 'east-1',
    });

    expect(entry).toBeNull();
    expect(readyHand).toBeNull();
    expect(selfDraw).toBeNull();
    expect(discardWin).toBeNull();
    expect(titleChange).toBe('🎉小C已由“初露锋芒”飞升为“至尊雀神”🍾');
  });
});
