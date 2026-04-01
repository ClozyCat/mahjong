import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import App from './App';

class MockWebSocket {
  static readonly CONNECTING = 0;
  static readonly OPEN = 1;
  static readonly CLOSING = 2;
  static readonly CLOSED = 3;
  static instances: MockWebSocket[] = [];

  readonly url: string;
  readyState = MockWebSocket.CONNECTING;
  onopen: (() => void) | null = null;
  onmessage: ((event: MessageEvent<string>) => void) | null = null;
  onerror: (() => void) | null = null;
  onclose: (() => void) | null = null;
  sentMessages: string[] = [];

  constructor(url: string) {
    this.url = url;
    MockWebSocket.instances.push(this);
  }

  send(message: string) {
    this.sentMessages.push(message);
  }

  close() {
    this.readyState = MockWebSocket.CLOSED;
    this.onclose?.();
  }

  triggerOpen() {
    this.readyState = MockWebSocket.OPEN;
    this.onopen?.();
  }

  triggerMessage(message: unknown) {
    this.onmessage?.({ data: JSON.stringify(message) } as MessageEvent<string>);
  }

  static reset() {
    MockWebSocket.instances = [];
  }
}

function createMockLocalStorage(): Storage {
  const store = new Map<string, string>();

  return {
    get length() {
      return store.size;
    },
    clear() {
      store.clear();
    },
    getItem(key: string) {
      return store.get(key) ?? null;
    },
    key(index: number) {
      return Array.from(store.keys())[index] ?? null;
    },
    removeItem(key: string) {
      store.delete(key);
    },
    setItem(key: string, value: string) {
      store.set(key, value);
    },
  };
}

function countSelectedTiles(container: HTMLElement) {
  return container.querySelectorAll('.mahjong-tile--selected').length;
}

function getLocalHandButtons() {
  const hand = screen.getByLabelText(/local hand/i);
  return Array.from(hand.querySelectorAll('button'));
}

function createPlayingSnapshotPayload(overrides: Record<string, unknown> = {}) {
  return {
    table_code: 'AB12CD',
    phase: 'playing',
    seats: [
      { seat_index: 0, nickname: 'Player A', connected: true, ready: true },
      { seat_index: 1, nickname: 'Player B', connected: true, ready: true },
      { seat_index: 2, nickname: 'Player C', connected: true, ready: true },
      { seat_index: 3, nickname: 'Player D', connected: true, ready: true },
    ],
    local_seat: 0,
    reconnect_token: 'token-1',
    match_state: {
      prevailing_wind: 'east',
      hand_number: 1,
      dealer_seat: 0,
      cumulative_scores: { '0': 0, '1': 0, '2': 0, '3': 0 },
      match_finished: false,
      last_completed_round_id: null,
    },
    private_state: {
      round_id: 'round-1',
      round_wind: 'east',
      dealer_seat: 0,
      current_actor: 0,
      last_discard: null,
      pending_action: {
        type: 'active_turn',
        seat_index: 0,
        deadline_at: '2026-03-27T12:00:00Z',
        drawn_tile_id: 'w1#1',
        options: ['discard'],
      },
      players: [
        {
          seat_index: 0,
          nickname: 'Player A',
          connected: true,
          concealed_count: 14,
          concealed_tiles: [{ tile_id: 'w1#1', tile_key: 'w1' }],
          melds: [],
          flowers: [],
          discards: [],
        },
        {
          seat_index: 1,
          nickname: 'Player B',
          connected: true,
          concealed_count: 13,
          melds: [],
          flowers: [],
          discards: [],
        },
        {
          seat_index: 2,
          nickname: 'Player C',
          connected: true,
          concealed_count: 13,
          melds: [],
          flowers: [],
          discards: [],
        },
        {
          seat_index: 3,
          nickname: 'Player D',
          connected: true,
          concealed_count: 13,
          melds: [],
          flowers: [],
          discards: [],
        },
      ],
    },
    ...overrides,
  };
}

async function joinTable(user: ReturnType<typeof userEvent.setup>) {
  render(<App />);

  await user.type(screen.getByLabelText('牌桌编号'), 'AB12CD');
  await user.type(screen.getByLabelText('昵称'), 'Player A');
  await user.click(screen.getByRole('button', { name: '加入牌桌' }));

  const socket = MockWebSocket.instances[0];
  expect(socket).toBeDefined();
  await act(async () => {
    socket!.triggerOpen();
  });

  return socket!;
}

describe('App', () => {
  beforeEach(() => {
    MockWebSocket.reset();
    const localStorageMock = createMockLocalStorage();

    vi.stubGlobal('localStorage', localStorageMock);
    Object.defineProperty(window, 'localStorage', {
      value: localStorageMock,
      configurable: true,
    });
    vi.stubGlobal('WebSocket', MockWebSocket);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it('picks a random zhongguose theme when the lobby opens', async () => {
    vi.spyOn(Math, 'random').mockReturnValue(0);

    render(<App />);

    await waitFor(() => {
      expect(document.documentElement.dataset.theme).toBe('tian-shui-bi');
    });
    expect(screen.getByText('当前配色')).toBeInTheDocument();
    expect(screen.getByText('天水碧')).toBeInTheDocument();
  });

  it('blocks create and join when the table code contains non-alphanumeric characters', async () => {
    const user = userEvent.setup();

    render(<App />);

    await user.type(screen.getByLabelText('牌桌编号'), 'ROOM-01');
    await user.type(screen.getByLabelText('昵称'), 'Player A');

    expect(screen.getAllByText('牌桌编号仅支持数字和英文字母。')).toHaveLength(2);
    expect(screen.getByRole('button', { name: '创建牌桌' })).toBeDisabled();
    expect(screen.getByRole('button', { name: '加入牌桌' })).toBeDisabled();
    expect(MockWebSocket.instances).toHaveLength(0);
  });

  it('leaves immediately without confirmation while the room is still waiting', async () => {
    const user = userEvent.setup();
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(true);

    render(<App />);

    await user.type(screen.getByLabelText('牌桌编号'), 'AB12CD');
    await user.type(screen.getByLabelText('昵称'), 'Player A');
    await user.click(screen.getByRole('button', { name: '加入牌桌' }));

    const socket = MockWebSocket.instances[0];
    expect(socket).toBeDefined();

    await act(async () => {
      socket.triggerOpen();
      socket.triggerMessage({
        type: 'room_snapshot',
        payload: {
          table_code: 'AB12CD',
          phase: 'waiting',
          seats: [{ seat_index: 0, nickname: 'Player A', connected: true, ready: false }],
          local_seat: 0,
          reconnect_token: 'token-1',
          match_state: null,
          private_state: null,
        },
      });
    });

    await user.click(await screen.findByRole('button', { name: '离开牌桌' }));

    expect(confirmSpy).not.toHaveBeenCalled();
    expect(socket.sentMessages.map((message) => JSON.parse(message))).toEqual([
      { type: 'join_table', payload: { nickname: 'Player A' } },
      { type: 'leave_table', payload: {} },
    ]);
  });

  it('still asks for confirmation after the match has started', async () => {
    const user = userEvent.setup();
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(true);

    const socket = await joinTable(user);

    await act(async () => {
      socket.triggerMessage({
        type: 'room_snapshot',
        payload: createPlayingSnapshotPayload(),
      });
    });

    await user.click(await screen.findByRole('button', { name: '快捷离开牌桌' }));

    expect(confirmSpy).toHaveBeenCalledWith('若主动离开，则无法再次加入对局，是否确定离开牌桌？');
    expect(socket.sentMessages.map((message) => JSON.parse(message))).toEqual([
      { type: 'join_table', payload: { nickname: 'Player A' } },
      { type: 'leave_table', payload: {} },
    ]);
  });

  it('returns to the lobby as soon as leave_table_accepted arrives', async () => {
    const user = userEvent.setup();
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    const socket = await joinTable(user);

    await act(async () => {
      socket.triggerMessage({
        type: 'room_snapshot',
        payload: createPlayingSnapshotPayload(),
      });
    });

    await user.click(await screen.findByRole('button', { name: '快捷离开牌桌' }));

    await act(async () => {
      socket.triggerMessage({
        type: 'leave_table_accepted',
        payload: {
          table_code: 'AB12CD',
          seat_index: 0,
        },
      });
    });

    expect(screen.getByRole('button', { name: '加入牌桌' })).toBeInTheDocument();
    expect(screen.queryByLabelText('Mahjong table')).toBeNull();
  });

  it('clears preselected claim tiles after passing', async () => {
    const user = userEvent.setup();
    const socket = await joinTable(user);

    await act(async () => {
      socket.triggerMessage({
        type: 'room_snapshot',
        payload: createPlayingSnapshotPayload({
          private_state: {
            round_id: 'round-1',
            round_wind: 'east',
            dealer_seat: 0,
            current_actor: 1,
            last_discard: 'w3',
            pending_action: {
              type: 'claim_window',
              discarder_seat: 1,
              deadline_at: '2026-03-30T12:00:00Z',
              responded_seats: [],
              options: ['chow', 'pass'],
            },
            players: [
              {
                seat_index: 0,
                nickname: 'Player A',
                connected: true,
                concealed_count: 13,
                concealed_tiles: [
                  { tile_id: 'w1#1', tile_key: 'w1' },
                  { tile_id: 'w2#2', tile_key: 'w2' },
                  { tile_id: 'b9#3', tile_key: 'b9' },
                ],
                melds: [],
                flowers: [],
                discards: [],
              },
              {
                seat_index: 1,
                nickname: 'Player B',
                connected: true,
                concealed_count: 13,
                melds: [],
                flowers: [],
                discards: [],
              },
              {
                seat_index: 2,
                nickname: 'Player C',
                connected: true,
                concealed_count: 13,
                melds: [],
                flowers: [],
                discards: [],
              },
              {
                seat_index: 3,
                nickname: 'Player D',
                connected: true,
                concealed_count: 13,
                melds: [],
                flowers: [],
                discards: [],
              },
            ],
          },
        }),
      });
    });

    expect(countSelectedTiles(document.body)).toBe(2);

    await user.click(screen.getByRole('button', { name: '过' }));
    expect(countSelectedTiles(document.body)).toBe(0);

    expect(socket.sentMessages.map((message) => JSON.parse(message))).toEqual([
      { type: 'join_table', payload: { nickname: 'Player A' } },
      { type: 'action_request', payload: { action_type: 'pass', tile_ids: [] } },
    ]);
  });

  it('clears preselected claim tiles after the claim window times out and play resumes', async () => {
    const user = userEvent.setup();
    const socket = await joinTable(user);

    await act(async () => {
      socket.triggerMessage({
        type: 'room_snapshot',
        payload: createPlayingSnapshotPayload({
          private_state: {
            round_id: 'round-1',
            round_wind: 'east',
            dealer_seat: 0,
            current_actor: 1,
            last_discard: 'w3',
            pending_action: {
              type: 'claim_window',
              discarder_seat: 1,
              deadline_at: '2026-03-30T12:00:00Z',
              responded_seats: [],
              options: ['chow', 'pass'],
            },
            players: [
              {
                seat_index: 0,
                nickname: 'Player A',
                connected: true,
                concealed_count: 13,
                concealed_tiles: [
                  { tile_id: 'w1#1', tile_key: 'w1' },
                  { tile_id: 'w2#2', tile_key: 'w2' },
                  { tile_id: 'b9#3', tile_key: 'b9' },
                ],
                melds: [],
                flowers: [],
                discards: [],
              },
              {
                seat_index: 1,
                nickname: 'Player B',
                connected: true,
                concealed_count: 13,
                melds: [],
                flowers: [],
                discards: [],
              },
              {
                seat_index: 2,
                nickname: 'Player C',
                connected: true,
                concealed_count: 13,
                melds: [],
                flowers: [],
                discards: [],
              },
              {
                seat_index: 3,
                nickname: 'Player D',
                connected: true,
                concealed_count: 13,
                melds: [],
                flowers: [],
                discards: [],
              },
            ],
          },
        }),
      });
    });

    expect(countSelectedTiles(document.body)).toBe(2);

    await act(async () => {
      socket.triggerMessage({
        type: 'room_snapshot',
        payload: createPlayingSnapshotPayload({
          private_state: {
            round_id: 'round-1',
            round_wind: 'east',
            dealer_seat: 0,
            current_actor: 2,
            last_discard: 'b7',
            pending_action: {
              type: 'active_turn',
              seat_index: 2,
              deadline_at: '2026-03-30T12:00:05Z',
              drawn_tile_id: 'b7#9',
              options: ['discard'],
            },
            players: [
              {
                seat_index: 0,
                nickname: 'Player A',
                connected: true,
                concealed_count: 13,
                concealed_tiles: [
                  { tile_id: 'w1#1', tile_key: 'w1' },
                  { tile_id: 'w2#2', tile_key: 'w2' },
                  { tile_id: 'b9#3', tile_key: 'b9' },
                ],
                melds: [],
                flowers: [],
                discards: [],
              },
              {
                seat_index: 1,
                nickname: 'Player B',
                connected: true,
                concealed_count: 13,
                melds: [],
                flowers: [],
                discards: [],
              },
              {
                seat_index: 2,
                nickname: 'Player C',
                connected: true,
                concealed_count: 14,
                melds: [],
                flowers: [],
                discards: [],
              },
              {
                seat_index: 3,
                nickname: 'Player D',
                connected: true,
                concealed_count: 13,
                melds: [],
                flowers: [],
                discards: [],
              },
            ],
          },
        }),
      });
    });

    expect(countSelectedTiles(document.body)).toBe(0);
  });

  it('replaces the previous single selection with the first claim candidate when the claim window opens', async () => {
    const user = userEvent.setup();
    const socket = await joinTable(user);

    await act(async () => {
      socket.triggerMessage({
        type: 'room_snapshot',
        payload: createPlayingSnapshotPayload({
          private_state: {
            round_id: 'round-1',
            round_wind: 'east',
            dealer_seat: 0,
            current_actor: 0,
            last_discard: null,
            pending_action: {
              type: 'active_turn',
              seat_index: 0,
              deadline_at: '2026-03-27T12:00:00Z',
              drawn_tile_id: 'w1#1',
              options: ['discard'],
            },
            players: [
              {
                seat_index: 0,
                nickname: 'Player A',
                connected: true,
                concealed_count: 14,
                concealed_tiles: [
                  { tile_id: 'w1#1', tile_key: 'w1' },
                  { tile_id: 'w2#2', tile_key: 'w2' },
                ],
                melds: [],
                flowers: [],
                discards: [],
              },
              {
                seat_index: 1,
                nickname: 'Player B',
                connected: true,
                concealed_count: 13,
                melds: [],
                flowers: [],
                discards: [],
              },
              {
                seat_index: 2,
                nickname: 'Player C',
                connected: true,
                concealed_count: 13,
                melds: [],
                flowers: [],
                discards: [],
              },
              {
                seat_index: 3,
                nickname: 'Player D',
                connected: true,
                concealed_count: 13,
                melds: [],
                flowers: [],
                discards: [],
              },
            ],
          },
        }),
      });
    });

    const handButtons = getLocalHandButtons();
    await user.click(handButtons[0]!);
    expect(countSelectedTiles(document.body)).toBe(1);

    await act(async () => {
      socket.triggerMessage({
        type: 'room_snapshot',
        payload: createPlayingSnapshotPayload({
          private_state: {
            round_id: 'round-1',
            round_wind: 'east',
            dealer_seat: 0,
            current_actor: 1,
            last_discard: 'w3',
            pending_action: {
              type: 'claim_window',
              discarder_seat: 1,
              deadline_at: '2026-03-30T12:00:00Z',
              responded_seats: [],
              options: ['chow', 'pass'],
            },
            players: [
              {
                seat_index: 0,
                nickname: 'Player A',
                connected: true,
                concealed_count: 13,
                concealed_tiles: [
                  { tile_id: 'w1#1', tile_key: 'w1' },
                  { tile_id: 'w2#2', tile_key: 'w2' },
                ],
                melds: [],
                flowers: [],
                discards: [],
              },
              {
                seat_index: 1,
                nickname: 'Player B',
                connected: true,
                concealed_count: 13,
                melds: [],
                flowers: [],
                discards: [],
              },
              {
                seat_index: 2,
                nickname: 'Player C',
                connected: true,
                concealed_count: 13,
                melds: [],
                flowers: [],
                discards: [],
              },
              {
                seat_index: 3,
                nickname: 'Player D',
                connected: true,
                concealed_count: 13,
                melds: [],
                flowers: [],
                discards: [],
              },
            ],
          },
        }),
      });
    });

    expect(countSelectedTiles(document.body)).toBe(2);
    expect(screen.getByRole('button', { name: '吃候选组合 1' })).toHaveAttribute('aria-pressed', 'true');
  });

  it('lets the player choose a claim candidate pane before confirming chow', async () => {
    const user = userEvent.setup();
    const socket = await joinTable(user);

    await act(async () => {
      socket.triggerMessage({
        type: 'room_snapshot',
        payload: createPlayingSnapshotPayload({
          private_state: {
            round_id: 'round-1',
            round_wind: 'east',
            dealer_seat: 0,
            current_actor: 1,
            last_discard: 'w3',
            pending_action: {
              type: 'claim_window',
              discarder_seat: 1,
              deadline_at: '2026-03-30T12:00:00Z',
              responded_seats: [],
              options: ['chow', 'pass'],
            },
            players: [
              {
                seat_index: 0,
                nickname: 'Player A',
                connected: true,
                concealed_count: 13,
                concealed_tiles: [
                  { tile_id: 'w1#1', tile_key: 'w1' },
                  { tile_id: 'w2#2', tile_key: 'w2' },
                  { tile_id: 'b9#3', tile_key: 'b9' },
                ],
                melds: [],
                flowers: [],
                discards: [],
              },
              {
                seat_index: 1,
                nickname: 'Player B',
                connected: true,
                concealed_count: 13,
                melds: [],
                flowers: [],
                discards: [],
              },
              {
                seat_index: 2,
                nickname: 'Player C',
                connected: true,
                concealed_count: 13,
                melds: [],
                flowers: [],
                discards: [],
              },
              {
                seat_index: 3,
                nickname: 'Player D',
                connected: true,
                concealed_count: 13,
                melds: [],
                flowers: [],
                discards: [],
              },
            ],
          },
        }),
      });
    });

    await user.click(await screen.findByRole('button', { name: '吃候选组合 1' }));
    expect(countSelectedTiles(document.body)).toBe(2);

    await user.click(screen.getByRole('button', { name: '吃' }));

    expect(countSelectedTiles(document.body)).toBe(0);
    expect(socket.sentMessages.map((message) => JSON.parse(message))).toEqual([
      { type: 'join_table', payload: { nickname: 'Player A' } },
      { type: 'action_request', payload: { action_type: 'chow', tile_ids: ['w1#1', 'w2#2'] } },
    ]);
  });

  it('submits the default selected chow pair when the chow button is clicked directly', async () => {
    const user = userEvent.setup();
    const socket = await joinTable(user);

    await act(async () => {
      socket.triggerMessage({
        type: 'room_snapshot',
        payload: createPlayingSnapshotPayload({
          private_state: {
            round_id: 'round-1',
            round_wind: 'east',
            dealer_seat: 0,
            current_actor: 1,
            last_discard: 'w3',
            pending_action: {
              type: 'claim_window',
              discarder_seat: 1,
              deadline_at: '2026-03-30T12:00:00Z',
              responded_seats: [],
              options: ['chow', 'pass'],
            },
            players: [
              {
                seat_index: 0,
                nickname: 'Player A',
                connected: true,
                concealed_count: 13,
                concealed_tiles: [
                  { tile_id: 'w1#1', tile_key: 'w1' },
                  { tile_id: 'w2#2', tile_key: 'w2' },
                  { tile_id: 'b9#3', tile_key: 'b9' },
                ],
                melds: [],
                flowers: [],
                discards: [],
              },
              {
                seat_index: 1,
                nickname: 'Player B',
                connected: true,
                concealed_count: 13,
                melds: [],
                flowers: [],
                discards: [],
              },
              {
                seat_index: 2,
                nickname: 'Player C',
                connected: true,
                concealed_count: 13,
                melds: [],
                flowers: [],
                discards: [],
              },
              {
                seat_index: 3,
                nickname: 'Player D',
                connected: true,
                concealed_count: 13,
                melds: [],
                flowers: [],
                discards: [],
              },
            ],
          },
        }),
      });
    });

    await user.click(screen.getByRole('button', { name: '吃' }));
    expect(countSelectedTiles(document.body)).toBe(0);
    expect(socket.sentMessages.map((message) => JSON.parse(message))).toEqual([
      { type: 'join_table', payload: { nickname: 'Player A' } },
      { type: 'action_request', payload: { action_type: 'chow', tile_ids: ['w1#1', 'w2#2'] } },
    ]);
  });

  it('supports double-clicking a hand tile to discard immediately', async () => {
    const user = userEvent.setup();
    const socket = await joinTable(user);

    await act(async () => {
      socket.triggerMessage({
        type: 'room_snapshot',
        payload: createPlayingSnapshotPayload({
          private_state: {
            round_id: 'round-1',
            round_wind: 'east',
            dealer_seat: 0,
            current_actor: 0,
            last_discard: null,
            pending_action: {
              type: 'active_turn',
              seat_index: 0,
              deadline_at: '2026-03-27T12:00:00Z',
              drawn_tile_id: 'w2#2',
              options: ['discard', 'kong'],
            },
            players: [
              {
                seat_index: 0,
                nickname: 'Player A',
                connected: true,
                concealed_count: 14,
                concealed_tiles: [
                  { tile_id: 'w1#1', tile_key: 'w1' },
                  { tile_id: 'w2#2', tile_key: 'w2' },
                ],
                melds: [],
                flowers: [],
                discards: [],
              },
              {
                seat_index: 1,
                nickname: 'Player B',
                connected: true,
                concealed_count: 13,
                melds: [],
                flowers: [],
                discards: [],
              },
              {
                seat_index: 2,
                nickname: 'Player C',
                connected: true,
                concealed_count: 13,
                melds: [],
                flowers: [],
                discards: [],
              },
              {
                seat_index: 3,
                nickname: 'Player D',
                connected: true,
                concealed_count: 13,
                melds: [],
                flowers: [],
                discards: [],
              },
            ],
          },
        }),
      });
    });

    await user.dblClick(getLocalHandButtons()[1]!);

    expect(countSelectedTiles(document.body)).toBe(0);
    expect(socket.sentMessages.map((message) => JSON.parse(message))).toEqual([
      { type: 'join_table', payload: { nickname: 'Player A' } },
      { type: 'action_request', payload: { action_type: 'discard', tile_ids: ['w2#2'] } },
    ]);
  });

  it('asks about kong before the normal turn flow when the local hand can self-kong', async () => {
    const user = userEvent.setup();
    const socket = await joinTable(user);

    await act(async () => {
      socket.triggerMessage({
        type: 'room_snapshot',
        payload: createPlayingSnapshotPayload({
          private_state: {
            round_id: 'round-1',
            round_wind: 'east',
            dealer_seat: 0,
            current_actor: 0,
            last_discard: null,
            pending_action: {
              type: 'active_turn',
              seat_index: 0,
              deadline_at: '2026-03-27T12:00:00Z',
              drawn_tile_id: 'w3#3',
              options: ['discard'],
            },
            players: [
              {
                seat_index: 0,
                nickname: 'Player A',
                connected: true,
                concealed_count: 5,
                concealed_tiles: [
                  { tile_id: 'w3#0', tile_key: 'w3' },
                  { tile_id: 'w3#1', tile_key: 'w3' },
                  { tile_id: 'w3#2', tile_key: 'w3' },
                  { tile_id: 'w3#3', tile_key: 'w3' },
                  { tile_id: 'b9#0', tile_key: 'b9' },
                ],
                melds: [],
                flowers: [],
                discards: [],
              },
              {
                seat_index: 1,
                nickname: 'Player B',
                connected: true,
                concealed_count: 13,
                melds: [],
                flowers: [],
                discards: [],
              },
              {
                seat_index: 2,
                nickname: 'Player C',
                connected: true,
                concealed_count: 13,
                melds: [],
                flowers: [],
                discards: [],
              },
              {
                seat_index: 3,
                nickname: 'Player D',
                connected: true,
                concealed_count: 13,
                melds: [],
                flowers: [],
                discards: [],
              },
            ],
          },
        }),
      });
    });

    expect(screen.getByRole('button', { name: '杠' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '过' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '出牌' })).toBeNull();
    expect(countSelectedTiles(document.body)).toBe(4);

    await user.click(screen.getByRole('button', { name: '过' }));

    expect(screen.queryByRole('button', { name: '过' })).toBeNull();
    expect(screen.queryByRole('button', { name: '杠' })).toBeNull();
    expect(screen.getByText('Player A正在执行操作：出牌')).toBeInTheDocument();
    expect(countSelectedTiles(document.body)).toBe(0);
    expect(socket.sentMessages.map((message) => JSON.parse(message))).toEqual([{ type: 'join_table', payload: { nickname: 'Player A' } }]);
  });

  it('still allows sending kong from the local kong-response prompt', async () => {
    const user = userEvent.setup();
    const socket = await joinTable(user);

    await act(async () => {
      socket.triggerMessage({
        type: 'room_snapshot',
        payload: createPlayingSnapshotPayload({
          private_state: {
            round_id: 'round-1',
            round_wind: 'east',
            dealer_seat: 0,
            current_actor: 0,
            last_discard: null,
            pending_action: {
              type: 'active_turn',
              seat_index: 0,
              deadline_at: '2026-03-27T12:00:00Z',
              drawn_tile_id: 'w3#3',
              options: ['discard'],
            },
            players: [
              {
                seat_index: 0,
                nickname: 'Player A',
                connected: true,
                concealed_count: 5,
                concealed_tiles: [
                  { tile_id: 'w3#0', tile_key: 'w3' },
                  { tile_id: 'w3#1', tile_key: 'w3' },
                  { tile_id: 'w3#2', tile_key: 'w3' },
                  { tile_id: 'w3#3', tile_key: 'w3' },
                  { tile_id: 'b9#0', tile_key: 'b9' },
                ],
                melds: [],
                flowers: [],
                discards: [],
              },
              {
                seat_index: 1,
                nickname: 'Player B',
                connected: true,
                concealed_count: 13,
                melds: [],
                flowers: [],
                discards: [],
              },
              {
                seat_index: 2,
                nickname: 'Player C',
                connected: true,
                concealed_count: 13,
                melds: [],
                flowers: [],
                discards: [],
              },
              {
                seat_index: 3,
                nickname: 'Player D',
                connected: true,
                concealed_count: 13,
                melds: [],
                flowers: [],
                discards: [],
              },
            ],
          },
        }),
      });
    });

    await user.click(screen.getByRole('button', { name: '杠' }));

    expect(socket.sentMessages.map((message) => JSON.parse(message))).toEqual([
      { type: 'join_table', payload: { nickname: 'Player A' } },
      { type: 'action_request', payload: { action_type: 'kong', tile_ids: ['w3#0', 'w3#1', 'w3#2', 'w3#3'] } },
    ]);
  });

  it('submits the matching claim action immediately when a candidate pane is double-clicked', async () => {
    const user = userEvent.setup();
    const socket = await joinTable(user);

    await act(async () => {
      socket.triggerMessage({
        type: 'room_snapshot',
        payload: createPlayingSnapshotPayload({
          private_state: {
            round_id: 'round-1',
            round_wind: 'east',
            dealer_seat: 0,
            current_actor: 1,
            last_discard: 'w3',
            pending_action: {
              type: 'claim_window',
              discarder_seat: 1,
              deadline_at: '2026-03-30T12:00:00Z',
              responded_seats: [],
              options: ['chow', 'pass'],
            },
            players: [
              {
                seat_index: 0,
                nickname: 'Player A',
                connected: true,
                concealed_count: 13,
                concealed_tiles: [
                  { tile_id: 'w1#1', tile_key: 'w1' },
                  { tile_id: 'w2#2', tile_key: 'w2' },
                  { tile_id: 'b9#3', tile_key: 'b9' },
                ],
                melds: [],
                flowers: [],
                discards: [],
              },
              {
                seat_index: 1,
                nickname: 'Player B',
                connected: true,
                concealed_count: 13,
                melds: [],
                flowers: [],
                discards: [],
              },
              {
                seat_index: 2,
                nickname: 'Player C',
                connected: true,
                concealed_count: 13,
                melds: [],
                flowers: [],
                discards: [],
              },
              {
                seat_index: 3,
                nickname: 'Player D',
                connected: true,
                concealed_count: 13,
                melds: [],
                flowers: [],
                discards: [],
              },
            ],
          },
        }),
      });
    });

    await user.dblClick(await screen.findByRole('button', { name: '吃候选组合 1' }));

    expect(countSelectedTiles(document.body)).toBe(0);
    expect(socket.sentMessages.map((message) => JSON.parse(message))).toEqual([
      { type: 'join_table', payload: { nickname: 'Player A' } },
      { type: 'action_request', payload: { action_type: 'chow', tile_ids: ['w1#1', 'w2#2'] } },
    ]);
  });
});
