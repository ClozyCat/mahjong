import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { UserProfilePanel } from './UserProfilePanel';

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
            multiplier: 2,
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
    expect(screen.getByText('x2 / 8 局')).toBeInTheDocument();
  });
});
