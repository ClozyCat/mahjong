import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import type { GameSummary, UserFanStat } from '../../types/match';
import { UserProfilePanel } from './UserProfilePanel';

describe('UserProfilePanel', () => {
  it('renders fan stats and recent games for a public user', () => {
    render(
      <UserProfilePanel
        user={{
          user_id: 1,
          username: 'alice',
          display_name: '阿明',
          points: 550,
          title: '熟练的码牌工',
          display_label: '阿明（熟练的码牌工）',
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
              points: 550,
              title: '熟练的码牌工',
              display_label: '阿明（熟练的码牌工）',
            },
            multiplier: 1,
            started_at: '2026-05-06T10:00:00Z',
            ended_at: '2026-05-06T11:00:00Z',
            round_count: 8,
            opponent_names: ['小李', '小王', '小陈'],
          },
        ]}
      />,
    );

    expect(screen.getByRole('heading', { name: '阿明（熟练的码牌工）' })).toBeInTheDocument();
    expect(screen.getByText('对对和')).toBeInTheDocument();
    expect(screen.getByText('4')).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: '历史牌局' })).toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: '最近牌局' })).not.toBeInTheDocument();
    expect(screen.getByText('05/06 19:00')).toBeInTheDocument();
    expect(screen.getByText('总 8 局')).toBeInTheDocument();
    expect(screen.getByText('小李、小王、小陈')).toBeInTheDocument();
    expect(screen.getByText('已经脱离了新手的低级趣味，不仅码牌速度快，甚至偶尔还能看懂别人在做什么牌。')).toBeInTheDocument();
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
          gameWithSummary(null, {
            game_id: 11,
            table_code: 'GAME01',
            opponent_names: ['对手1'],
          }),
          gameWithSummary(null, {
            game_id: 12,
            table_code: 'GAME02',
            opponent_names: ['对手2'],
          }),
          gameWithSummary(null, {
            game_id: 13,
            table_code: 'GAME03',
            opponent_names: ['对手3'],
          }),
          gameWithSummary(null, {
            game_id: 14,
            table_code: 'GAME04',
            opponent_names: ['对手4'],
          }),
        ]}
      />,
    );

    expect(screen.getByText('对手1')).toBeInTheDocument();
    expect(screen.getByText('对手3')).toBeInTheDocument();
    expect(screen.queryByText('对手4')).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '历史牌局分页下一页' }));

    expect(screen.queryByText('对手1')).not.toBeInTheDocument();
    expect(screen.getByText('对手4')).toBeInTheDocument();
  });

  it('shows score delta and opponent names directly in a history row', () => {
    render(
      <UserProfilePanel
        user={{
          user_id: 1,
          username: 'alice',
          display_name: '阿明',
          points: 550,
          title: '熟练的码牌工',
          display_label: '阿明（熟练的码牌工）',
          bio: '',
          avatar: null,
        }}
        fanStats={[]}
        recentGames={[
          gameWithSummary(
            {
              round_count: 8,
              win_count: 3,
              self_draw_win_count: 2,
              discard_win_count: 1,
              deal_in_count: 1,
              total_score_delta: 28,
              average_cumulative_score: 14,
              high_score_round_count: 2,
            },
            { opponent_names: ['Guest A', 'Guest B', 'Guest C'] },
          ),
        ]}
      />,
    );

    expect(screen.getByText('+28')).toBeInTheDocument();
    expect(screen.getByText('Guest A、Guest B、Guest C')).toBeInTheDocument();
    expect(screen.queryByRole('tooltip')).not.toBeInTheDocument();
  });

  it('keeps public bio tied to title instead of recent records', () => {
    render(
      <UserProfilePanel
        user={publicUser({
          title: '弹性拆牌艺术家',
          display_label: '阿明（弹性拆牌艺术家）',
        })}
        fanStats={fanStats()}
        recentGames={[
          gameWithSummary({
            round_count: 8,
            win_count: 4,
            high_score_round_count: 5,
            deal_in_count: 3,
          }),
        ]}
      />,
    );

    expect(screen.getByText(/深谙“好死不如赖活着”的麻将哲学/)).toBeInTheDocument();
    expect(screen.queryByText('放铳王')).not.toBeInTheDocument();
    expect(screen.queryByText('雀圣')).not.toBeInTheDocument();
  });
});

function gameWithSummary(
  playerSummary: Partial<NonNullable<GameSummary['player_summary']>> | null,
  overrides: Partial<Pick<GameSummary, 'game_id' | 'table_code' | 'round_count' | 'opponent_names'>> = {},
): GameSummary {
  return {
    game_id: overrides.game_id ?? 11,
    table_code: overrides.table_code ?? 'AB12CD',
    owner: publicUser(),
    multiplier: 1,
    started_at: '2026-05-06T10:00:00Z',
    ended_at: '2026-05-06T11:00:00Z',
    round_count: overrides.round_count ?? 8,
    opponent_names: overrides.opponent_names ?? [],
    player_summary: playerSummary ? { ...emptyPlayerSummary(), ...playerSummary } : null,
  };
}

function publicUser(overrides: Partial<ReturnType<typeof basePublicUser>> = {}) {
  return { ...basePublicUser(), ...overrides };
}

function basePublicUser() {
  return {
    user_id: 1,
    username: 'alice',
    display_name: '阿明',
    points: 550,
    title: '熟练的码牌工',
    display_label: '阿明（熟练的码牌工）',
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
