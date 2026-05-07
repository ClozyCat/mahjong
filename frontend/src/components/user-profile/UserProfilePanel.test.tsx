import { fireEvent, render, screen } from '@testing-library/react';
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
    expect(screen.getByText('对对和')).toBeInTheDocument();
    expect(screen.getByText('4')).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: '历史牌局' })).toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: '最近牌局' })).not.toBeInTheDocument();
    expect(screen.getByText('AB12CD')).toBeInTheDocument();
    expect(screen.getByText('8 局')).toBeInTheDocument();
    expect(screen.getByText('暂无公开简介')).toBeInTheDocument();
    expect(screen.queryByText(/x[123]/)).not.toBeInTheDocument();
  });

  it('paginates fan stats with five rows per page', () => {
    render(
      <UserProfilePanel
        user={publicUser()}
        fanStats={[
          {
            user_id: 1,
            fan_key: 'all_pungs',
            fan_label: 'all_pungs',
            count: 4,
            last_seen_at: '2026-05-06T12:00:00Z',
          },
          ...Array.from({ length: 5 }, (_, index) => ({
            user_id: 1,
            fan_key: `custom_fan_${index + 2}`,
            fan_label: `自定义番${index + 2}`,
            count: index + 2,
            last_seen_at: '2026-05-06T12:00:00Z',
          })),
        ]}
        recentGames={[]}
      />,
    );

    expect(screen.getByText('对对和')).toBeInTheDocument();
    expect(screen.getByText('自定义番5')).toBeInTheDocument();
    expect(screen.queryByText('自定义番6')).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '番种统计分页下一页' }));

    expect(screen.queryByText('对对和')).not.toBeInTheDocument();
    expect(screen.getByText('自定义番6')).toBeInTheDocument();
  });

  it('paginates history games with three rows per page', () => {
    render(
      <UserProfilePanel
        user={publicUser()}
        fanStats={[]}
        recentGames={[
          gameWithSummary(null, { game_id: 11, table_code: 'GAME01' }),
          gameWithSummary(null, { game_id: 12, table_code: 'GAME02' }),
          gameWithSummary(null, { game_id: 13, table_code: 'GAME03' }),
          gameWithSummary(null, { game_id: 14, table_code: 'GAME04' }),
        ]}
      />,
    );

    expect(screen.getByText('GAME01')).toBeInTheDocument();
    expect(screen.getByText('GAME03')).toBeInTheDocument();
    expect(screen.queryByText('GAME04')).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '历史牌局分页下一页' }));

    expect(screen.queryByText('GAME01')).not.toBeInTheDocument();
    expect(screen.getByText('GAME04')).toBeInTheDocument();
  });

  it('shows the final result popover when hovering a recent game', () => {
    render(
      <UserProfilePanel
        user={{
          user_id: 1,
          username: 'alice',
          display_name: '阿明',
          points: 320,
          title: '平民',
          display_label: '阿明（平民）',
          bio: '',
          avatar: null,
        }}
        fanStats={[]}
        recentGames={[
          gameWithSummary({
            round_count: 8,
            win_count: 3,
            self_draw_win_count: 2,
            discard_win_count: 1,
            deal_in_count: 1,
            total_score_delta: 28,
            average_cumulative_score: 14,
            high_score_round_count: 2,
          }),
        ]}
      />,
    );

    expect(screen.queryByRole('tooltip', { name: 'AB12CD 最终结果' })).not.toBeInTheDocument();

    fireEvent.mouseEnter(screen.getByText('AB12CD').closest('li')!);

    expect(screen.getByRole('tooltip', { name: 'AB12CD 最终结果' })).toBeInTheDocument();
    expect(screen.getByText('+28')).toBeInTheDocument();
    expect(screen.getByText('3 胜 / 1 放铳')).toBeInTheDocument();
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
  overrides: Partial<Pick<GameSummary, 'game_id' | 'table_code' | 'round_count'>> = {},
): GameSummary {
  return {
    game_id: overrides.game_id ?? 11,
    table_code: overrides.table_code ?? 'AB12CD',
    owner: publicUser(),
    multiplier: 1,
    started_at: '2026-05-06T10:00:00Z',
    ended_at: '2026-05-06T11:00:00Z',
    round_count: overrides.round_count ?? 8,
    player_summary: playerSummary ? { ...emptyPlayerSummary(), ...playerSummary } : null,
  };
}

function publicUser() {
  return {
    user_id: 1,
    username: 'alice',
    display_name: '阿明',
    points: 320,
    title: '平民',
    display_label: '阿明（平民）',
    bio: '喜欢朋友局。',
    avatar: null,
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
      fan_label: 'all_pungs',
      count: 4,
      last_seen_at: '2026-05-06T12:00:00Z',
    },
  ];
}
