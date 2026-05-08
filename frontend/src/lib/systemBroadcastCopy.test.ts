import { describe, expect, it } from 'vitest';

import {
  createPresenceSystemBroadcast,
  createRoundEventSystemBroadcast,
  createTitleChangeSystemBroadcast,
  titleForPoints,
} from './systemBroadcastCopy';

const seats = [
  { seat_index: 0, user_id: 10, nickname: '小A', points: 550, title: '熟练的码牌工', connected: true, ready: true },
  { seat_index: 1, user_id: 11, nickname: '小B', points: -20, title: '全自动点炮机', connected: true, ready: true },
  { seat_index: 2, user_id: 12, nickname: '小C', points: 1350, title: '言出法随真雀神', connected: true, ready: true },
];

function expectSystemPrefix(copy: string | null) {
  expect(copy).toMatch(/^\p{Emoji}系统播报：/u);
}

describe('system broadcast copy', () => {
  it('maps player titles with 100-point lower-inclusive bands', () => {
    expect(titleForPoints(-1000)).toBe('全自动点炮机');
    expect(titleForPoints(49)).toBe('全自动点炮机');
    expect(titleForPoints(50)).toBe('首席散财童子');
    expect(titleForPoints(550)).toBe('熟练的码牌工');
    expect(titleForPoints(649)).toBe('熟练的码牌工');
    expect(titleForPoints(650)).toBe('弹性拆牌艺术家');
    expect(titleForPoints(9999)).toBe('言出法随真雀神');
  });

  it('creates a neutral ready-hand barrage with player title', () => {
    const copy = createRoundEventSystemBroadcast('ready_hand_declared', { seat: 0 }, seats);

    expectSystemPrefix(copy);
    expect(copy).toContain('小A（熟练的码牌工）宣布听牌');
  });

  it('uses playful low-score copy for player entry', () => {
    const copy = createPresenceSystemBroadcast(
      { table_code: 'ROOM1', seat_index: 1, connected: true },
      seats,
    );

    expectSystemPrefix(copy);
    expect(copy).toContain('小B（全自动点炮机）');
    expect(copy).toContain('节目效果');
  });

  it('uses worshipful high-score copy for self draw', () => {
    const copy = createRoundEventSystemBroadcast('self_hu_declared', { seat: 2 }, seats);

    expect(copy?.startsWith('✨系统播报：')).toBe(true);
    expect(copy).toContain('小C（言出法随真雀神）自摸成功');
  });

  it('announces discard wins as deal-in broadcasts with both titles', () => {
    const copy = createRoundEventSystemBroadcast(
      'claim_made',
      { seat: 0, from: 1, claim_type: 'hu' },
      seats,
    );

    expectSystemPrefix(copy);
    expect(copy).toContain('小B（全自动点炮机）给小A（熟练的码牌工）放铳');
  });

  it('announces title changes with a dynamic event emoji', () => {
    const copy = createTitleChangeSystemBroadcast({
      user_id: 10,
      display_name: '小A',
      delta: 20,
      old_points: 540,
      points: 560,
      old_title: '间歇性好运携带者',
      title: '熟练的码牌工',
      reason: 'round_settlement',
      source_table_code: 'ROOM1',
      source_round_id: 'east-1',
    });

    expect(copy?.startsWith('📣系统播报：')).toBe(true);
    expect(copy).toContain('小A（熟练的码牌工）成功由“间歇性好运携带者”晋升“熟练的码牌工”');
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
      old_title: '弹性拆牌艺术家',
      title: '言出法随真雀神',
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
