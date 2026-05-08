import { describe, expect, it } from 'vitest';

import {
  createPresenceSystemBroadcast,
  createRoundEventSystemBroadcast,
  createTitleChangeSystemBroadcast,
} from './systemBroadcastCopy';

const seats = [
  { seat_index: 0, user_id: 10, nickname: '小A', points: 650, title: '概率论博导', connected: true, ready: true },
  { seat_index: 1, user_id: 11, nickname: '小B', points: -20, title: '赛博 ATM', connected: true, ready: true },
  { seat_index: 2, user_id: 12, nickname: '小C', points: 1300, title: '只手遮天大魔王', connected: true, ready: true },
];

function expectSystemPrefix(copy: string | null) {
  expect(copy).toMatch(/^\p{Emoji}系统播报：/u);
}

describe('system broadcast copy', () => {
  it('creates a neutral ready-hand barrage with player title', () => {
    const copy = createRoundEventSystemBroadcast('ready_hand_declared', { seat: 0 }, seats);

    expectSystemPrefix(copy);
    expect(copy).toContain('小A（概率论博导）宣布听牌');
  });

  it('uses playful low-score copy for player entry', () => {
    const copy = createPresenceSystemBroadcast(
      { table_code: 'ROOM1', seat_index: 1, connected: true },
      seats,
    );

    expectSystemPrefix(copy);
    expect(copy).toContain('小B（赛博 ATM）');
    expect(copy).toContain('节目效果');
  });

  it('uses worshipful high-score copy for self draw', () => {
    const copy = createRoundEventSystemBroadcast('self_hu_declared', { seat: 2 }, seats);

    expect(copy?.startsWith('✨系统播报：')).toBe(true);
    expect(copy).toContain('小C（只手遮天大魔王）自摸成功');
  });

  it('announces discard wins as deal-in broadcasts with both titles', () => {
    const copy = createRoundEventSystemBroadcast(
      'claim_made',
      { seat: 0, from: 1, claim_type: 'hu' },
      seats,
    );

    expectSystemPrefix(copy);
    expect(copy).toContain('小B（赛博 ATM）给小A（概率论博导）放铳');
  });

  it('announces title changes with a dynamic event emoji', () => {
    const copy = createTitleChangeSystemBroadcast({
      user_id: 10,
      display_name: '小A',
      delta: 20,
      old_points: 590,
      points: 610,
      old_title: '正分守门员',
      title: '概率论博导',
      reason: 'round_settlement',
      source_table_code: 'ROOM1',
      source_round_id: 'east-1',
    });

    expect(copy?.startsWith('📣系统播报：')).toBe(true);
    expect(copy).toContain('小A（概率论博导）成功由“正分守门员”晋升“概率论博导”');
  });

  it('uses event-specific emoji instead of reusing the crown example', () => {
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
      old_title: '概率论博导',
      title: '只手遮天大魔王',
      reason: 'round_settlement',
      source_table_code: 'ROOM1',
      source_round_id: 'east-1',
    });

    expect(entry?.startsWith('🌟系统播报：')).toBe(true);
    expect(readyHand?.startsWith('🔮系统播报：')).toBe(true);
    expect(selfDraw?.startsWith('✨系统播报：')).toBe(true);
    expect(discardWin?.startsWith('⚡系统播报：')).toBe(true);
    expect(titleChange?.startsWith('🏆系统播报：')).toBe(true);
  });
});
