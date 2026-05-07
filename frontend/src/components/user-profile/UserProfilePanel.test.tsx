import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import type { GameSummary, UserFanStat } from '../../types/match';
import { UserProfilePanel, generatePublicBio } from './UserProfilePanel';

describe('UserProfilePanel', () => {
  it('renders fan stats and recent games for a public user', () => {
    render(
      <UserProfilePanel
        user={{
          user_id: 1,
          username: 'alice',
          display_name: '阿明',
          points: 320,
          title: '平民',
          display_label: '阿明（平民）',
          bio: '喜欢朋友局。',
          avatar: null,
        }}
        fanStats={[
          {
            user_id: 1,
            fan_key: 'all_pungs',
            fan_label: '碰碰和',
            count: 4,
            last_seen_at: '2026-05-06T12:00:00Z',
          },
        ]}
        recentGames={[
          {
            game_id: 11,
            table_code: 'AB12CD',
            owner: {
              user_id: 1,
              display_name: '阿明',
              points: 320,
              title: '平民',
              display_label: '阿明（平民）',
            },
            multiplier: 1,
            started_at: '2026-05-06T10:00:00Z',
            ended_at: '2026-05-06T11:00:00Z',
            round_count: 8,
          },
        ]}
      />,
    );

    expect(screen.getByRole('heading', { name: '阿明（平民）' })).toBeInTheDocument();
    expect(screen.getByText('碰碰和')).toBeInTheDocument();
    expect(screen.getByText('4')).toBeInTheDocument();
    expect(screen.getByText('AB12CD')).toBeInTheDocument();
    expect(screen.getByText('8 局')).toBeInTheDocument();
    expect(screen.getByText('暂无公开简介')).toBeInTheDocument();
    expect(screen.queryByText(/x[123]/)).not.toBeInTheDocument();
  });

  it('generates a deal-in bio from recent player summaries', () => {
    expect(
      generatePublicBio([gameWithSummary({ round_count: 8, deal_in_count: 3 })], []),
    ).toBe('放铳王');
  });

  it('generates a top winner bio when wins and scores stay high', () => {
    expect(
      generatePublicBio([
        gameWithSummary({
          round_count: 8,
          win_count: 4,
          self_draw_win_count: 2,
          high_score_round_count: 5,
          total_score_delta: 160,
          average_cumulative_score: 120,
        }),
      ], []),
    ).toBe('雀圣');
  });

  it('keeps the empty public bio when there are no player records', () => {
    expect(generatePublicBio([], fanStats())).toBe('暂无公开简介');
    expect(generatePublicBio([gameWithSummary(null)], fanStats())).toBe('暂无公开简介');
  });
});

function gameWithSummary(
  playerSummary: Partial<NonNullable<GameSummary['player_summary']>> | null,
): GameSummary {
  return {
    game_id: 11,
    table_code: 'AB12CD',
    owner: {
      user_id: 1,
      display_name: '阿明',
      points: 320,
      title: '平民',
      display_label: '阿明（平民）',
    },
    multiplier: 1,
    started_at: '2026-05-06T10:00:00Z',
    ended_at: '2026-05-06T11:00:00Z',
    round_count: 8,
    player_summary: playerSummary ? { ...emptyPlayerSummary(), ...playerSummary } : null,
  };
}

function emptyPlayerSummary(): NonNullable<GameSummary['player_summary']> {
  return {
    round_count: 0,
    win_count: 0,
    self_draw_win_count: 0,
    discard_win_count: 0,
    deal_in_count: 0,
    total_score_delta: 0,
    average_cumulative_score: 0,
    high_score_round_count: 0,
  };
}

function fanStats(): UserFanStat[] {
  return [
    {
      user_id: 1,
      fan_key: 'all_pungs',
      fan_label: '碰碰和',
      count: 4,
      last_seen_at: '2026-05-06T12:00:00Z',
    },
  ];
}
