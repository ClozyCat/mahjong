import { act, render, screen } from '@testing-library/react';
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
        },
      });
    });

    await user.click(await screen.findByRole('button', { name: '离开牌桌' }));

    expect(confirmSpy).toHaveBeenCalledWith('若主动离开，则无法再次加入对局，是否确定离开牌桌？');
    expect(socket.sentMessages.map((message) => JSON.parse(message))).toEqual([
      { type: 'join_table', payload: { nickname: 'Player A' } },
      { type: 'leave_table', payload: {} },
    ]);
  });

  it('clears preselected claim tiles after passing', async () => {
    const user = userEvent.setup();
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
        },
      });
    });

    await user.click(await screen.findByRole('button', { name: '吃' }));
    expect(countSelectedTiles(document.body)).toBe(2);

    await user.click(screen.getByRole('button', { name: '过' }));
    expect(countSelectedTiles(document.body)).toBe(0);

    expect(socket.sentMessages.map((message) => JSON.parse(message))).toEqual([
      { type: 'join_table', payload: { nickname: 'Player A' } },
      { type: 'action_request', payload: { action_type: 'pass', tile_ids: [] } },
    ]);
  });
});
