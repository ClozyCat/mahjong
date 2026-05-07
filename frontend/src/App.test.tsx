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

function mockBackgroundMusicPlayback() {
  const play = vi.fn(() => Promise.resolve());
  const pause = vi.fn();
  const addEventListener = vi.fn();
  const audio = vi.fn((url: string) => {
    void url;
    return {
      play,
      pause,
      addEventListener,
    };
  });
  const originalAudio = globalThis.Audio;

  globalThis.Audio = audio as unknown as typeof Audio;

  return {
    audio,
    play,
    pause,
    restore: () => {
      globalThis.Audio = originalAudio;
    },
  };
}

function countSelectedTiles(container: HTMLElement) {
  return container.querySelectorAll('.mahjong-tile--selected').length;
}

function countRelatedHighlightTiles(container: HTMLElement) {
  return container.querySelectorAll('.mahjong-tile--related-highlight').length;
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

function mockMobileBattleImmersiveApis() {
  const lock = vi.fn().mockResolvedValue(undefined);
  const requestFullscreen = vi.fn().mockImplementation(async () => {
    fullscreenElement = document.documentElement;
    document.dispatchEvent(new Event('fullscreenchange'));
  });
  const exitFullscreen = vi.fn().mockImplementation(async () => {
    fullscreenElement = null;
    document.dispatchEvent(new Event('fullscreenchange'));
  });
  let fullscreenElement: Element | null = null;

  Object.defineProperty(window.navigator, 'userAgent', {
    configurable: true,
    value: 'Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X)',
  });
  Object.defineProperty(window, 'innerWidth', {
    configurable: true,
    writable: true,
    value: 390,
  });
  Object.defineProperty(window, 'innerHeight', {
    configurable: true,
    writable: true,
    value: 844,
  });
  Object.defineProperty(window.screen, 'orientation', {
    configurable: true,
    value: { lock },
  });
  Object.defineProperty(document, 'fullscreenElement', {
    configurable: true,
    get: () => fullscreenElement,
  });
  Object.defineProperty(document.documentElement, 'requestFullscreen', {
    configurable: true,
    value: requestFullscreen,
  });
  Object.defineProperty(document, 'exitFullscreen', {
    configurable: true,
    value: exitFullscreen,
  });

  return { lock, requestFullscreen, exitFullscreen };
}

const AUTH_SESSION_TOKEN = 'auth-session-1';
const DEFAULT_CURRENT_USER = {
  user_id: 1,
  username: 'player-a',
  display_name: 'Player A',
  points: 150,
  title: '平民',
  display_label: 'Player A（平民）',
  bio: '',
  avatar: null,
};
const DEFAULT_LEADERBOARD = [
  DEFAULT_CURRENT_USER,
  {
    user_id: 2,
    username: 'player-b',
    display_name: 'Player B',
    points: 90,
    title: '平民',
    display_label: 'Player B（平民）',
    bio: '',
    avatar: null,
  },
];
const DEFAULT_PENDING_INVITE = {
  id: 7,
  table_code: 'ZXCVBN',
  inviter_user_id: 2,
  invitee_user_id: 1,
  status: 'pending',
  created_at: '2026-05-06T12:00:00Z',
  expires_at: '2026-05-06T12:10:00Z',
};

function createMockResponse(body: unknown, status = 200): Response {
  return {
    ok: status >= 200 && status < 300,
    status,
    statusText: status === 204 ? 'No Content' : 'OK',
    json: async () => body,
    text: async () => (body === undefined ? '' : JSON.stringify(body)),
  } as Response;
}

function parseRequestBody(init?: RequestInit) {
  if (!init?.body || typeof init.body !== 'string') {
    return null;
  }

  return JSON.parse(init.body);
}

function seedStoredAuthSession(user = DEFAULT_CURRENT_USER) {
  localStorage.setItem(
    'mahjong:auth',
    JSON.stringify({
      sessionToken: AUTH_SESSION_TOKEN,
      user,
    }),
  );
}

function createFetchMock(options?: {
  me?: typeof DEFAULT_CURRENT_USER;
  leaderboard?: typeof DEFAULT_LEADERBOARD;
  invites?: typeof DEFAULT_PENDING_INVITE[];
  createdTableCode?: string;
  acceptInviteStatus?: number;
  acceptInviteDetail?: string;
}) {
  const me = options?.me ?? DEFAULT_CURRENT_USER;
  const leaderboard = options?.leaderboard ?? DEFAULT_LEADERBOARD;
  const invites = options?.invites ?? [];
  const createdTableCode = options?.createdTableCode ?? 'AB12CD';
  const acceptInviteStatus = options?.acceptInviteStatus ?? 200;
  const acceptInviteDetail = options?.acceptInviteDetail;

  return vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    const method = init?.method ?? 'GET';

    if (url.endsWith('/api/auth/login') && method === 'POST') {
      return createMockResponse({
        session_token: AUTH_SESSION_TOKEN,
        user: me,
      });
    }

    if (url.endsWith('/api/auth/register') && method === 'POST') {
      const body = parseRequestBody(init);
      const displayName = String(body?.display_name ?? me.display_name);
      return createMockResponse({
        session_token: AUTH_SESSION_TOKEN,
        user: {
          ...me,
          username: displayName,
          display_name: displayName,
          display_label: `${displayName}（${me.title}）`,
        },
      });
    }

    if (url.endsWith('/api/auth/logout') && method === 'POST') {
      return createMockResponse(undefined, 204);
    }

    if (url.endsWith('/api/me') && method === 'GET') {
      return createMockResponse(me);
    }

    if (url.endsWith('/api/me/invites') && method === 'GET') {
      return createMockResponse(invites);
    }

    if (url.endsWith('/api/me/spectator-requests') && method === 'GET') {
      return createMockResponse([]);
    }

    if (url.endsWith('/api/leaderboard') && method === 'GET') {
      return createMockResponse(leaderboard);
    }

    if (/\/api\/users\/\d+\/fans$/.test(url) && method === 'GET') {
      return createMockResponse([]);
    }

    if (/\/api\/users\/\d+\/games$/.test(url) && method === 'GET') {
      return createMockResponse([]);
    }

    if (url.endsWith('/api/tables') && method === 'POST') {
      return createMockResponse({
        table_code: createdTableCode,
        phase: 'waiting',
        owner_user_id: me.user_id,
        multiplier: parseRequestBody(init)?.multiplier ?? 1,
        created_at: '2026-05-06T12:00:00Z',
        seats: [],
      });
    }

    if (/\/api\/tables\/[^/]+\/invites$/.test(url) && method === 'POST') {
      const body = parseRequestBody(init);
      return createMockResponse({
        ...DEFAULT_PENDING_INVITE,
        table_code: createdTableCode,
        inviter_user_id: me.user_id,
        invitee_user_id: Number(body?.invitee_user_id ?? DEFAULT_PENDING_INVITE.invitee_user_id),
      });
    }

    if (/\/api\/invites\/\d+\/accept$/.test(url) && method === 'POST') {
      if (acceptInviteStatus < 200 || acceptInviteStatus >= 300) {
        return createMockResponse({ detail: acceptInviteDetail ?? 'table_invite_invalid' }, acceptInviteStatus);
      }

      return createMockResponse({
        invite_id: DEFAULT_PENDING_INVITE.id,
        table_code: DEFAULT_PENDING_INVITE.table_code,
        seat_index: 1,
        status: 'accepted',
      });
    }

    throw new Error(`Unhandled fetch request: ${method} ${url}`);
  });
}

function findFetchCall(fetchMock: ReturnType<typeof vi.fn>, path: string, method = 'GET') {
  return fetchMock.mock.calls.find(
    ([input, init]) => String(input).endsWith(path) && (init?.method ?? 'GET') === method,
  );
}

function getMeSocket() {
  return MockWebSocket.instances.find((socket) => socket.url.includes('/ws/me'));
}

function getRoomSocket(tableCode = 'AB12CD') {
  return MockWebSocket.instances.find((socket) => socket.url.endsWith(`/ws/${tableCode}`));
}

function getRoomSockets(tableCode = 'AB12CD') {
  return MockWebSocket.instances.filter((socket) => socket.url.endsWith(`/ws/${tableCode}`));
}

function expectTableHome() {
  expect(screen.getByLabelText('Mahjong table')).toBeInTheDocument();
  expect(screen.getByRole('complementary', { name: 'Table sidebar' })).toBeInTheDocument();
  expect(screen.getByRole('region', { name: '牌桌侧栏首页' })).toBeInTheDocument();
  expect(screen.getByRole('button', { name: '创建牌局' })).toBeInTheDocument();
}

async function renderAuthenticatedLobby(
  options?: Parameters<typeof createFetchMock>[0] & {
    user?: typeof DEFAULT_CURRENT_USER;
  },
) {
  const fetchMock = createFetchMock(options);
  vi.stubGlobal('fetch', fetchMock);
  seedStoredAuthSession(options?.user ?? options?.me ?? DEFAULT_CURRENT_USER);

  render(<App />);

  await screen.findByRole('heading', { name: (options?.me ?? DEFAULT_CURRENT_USER).display_label });
  await waitFor(() => {
    expect(getMeSocket()).toBeDefined();
  });

  return { fetchMock };
}

async function joinTable(
  user: ReturnType<typeof userEvent.setup>,
  options?: Parameters<typeof renderAuthenticatedLobby>[0] & {
    multiplier?: 1 | 2 | 3;
  },
) {
  const lobby = await renderAuthenticatedLobby(options);

  if (options?.multiplier && options.multiplier !== 1) {
    await user.click(screen.getByRole('button', { name: `x${options.multiplier}` }));
  }

  await user.click(screen.getByRole('button', { name: '创建牌局' }));
  await waitFor(() => {
    expect(getRoomSocket(options?.createdTableCode ?? 'AB12CD')).toBeDefined();
  });

  const socket = getRoomSocket(options?.createdTableCode ?? 'AB12CD');
  expect(socket).toBeDefined();
  await act(async () => {
    socket!.triggerOpen();
  });

  return {
    ...lobby,
    socket: socket!,
  };
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
    Object.defineProperty(window, 'innerWidth', {
      configurable: true,
      writable: true,
      value: 1024,
    });
    Object.defineProperty(window, 'innerHeight', {
      configurable: true,
      writable: true,
      value: 768,
    });
  });

  it('does not request landscape orientation when a mobile user joins a table', async () => {
    const user = userEvent.setup();
    const { lock } = mockMobileBattleImmersiveApis();

    await joinTable(user);

    expect(lock).not.toHaveBeenCalled();
  });

  it('shows login and registration tabs when unauthenticated', () => {
    render(<App />);

    expect(screen.getByRole('tab', { name: '登录' })).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByRole('tab', { name: '邀请码注册' })).toBeInTheDocument();
    expect(screen.getByLabelText('账号或用户 ID')).toBeInTheDocument();
  });

  it('does not start background music from table-home interactions', () => {
    localStorage.setItem('mahjong:bgm', 'true');
    const audioMock = mockBackgroundMusicPlayback();

    try {
      render(<App />);

      window.dispatchEvent(new Event('pointerdown'));

      expect(audioMock.audio).not.toHaveBeenCalled();
      expect(audioMock.play).not.toHaveBeenCalled();
    } finally {
      audioMock.restore();
    }
  });

  it('lets a bot-takeover player start the match after all seats are bots', async () => {
    const user = userEvent.setup();
    const { socket } = await joinTable(user);

    await act(async () => {
      socket.triggerMessage({
        type: 'room_snapshot',
        payload: {
          table_code: 'AB12CD',
          phase: 'waiting',
          seats: [
            { seat_index: 0, nickname: 'Player A', connected: true, ready: true, is_bot: true, seat_type: 'human' },
            { seat_index: 1, nickname: 'Bot 1', connected: true, ready: true, is_bot: true, seat_type: 'bot' },
            { seat_index: 2, nickname: 'Bot 2', connected: true, ready: true, is_bot: true, seat_type: 'bot' },
            { seat_index: 3, nickname: 'Bot 3', connected: true, ready: true, is_bot: true, seat_type: 'bot' },
          ],
          local_seat: 0,
          reconnect_token: 'token-1',
        },
      });
    });

    await user.click(await screen.findByRole('button', { name: '开始对局' }));

    expect(socket.sentMessages.map((message) => JSON.parse(message))).toEqual([
      { type: 'join_table', payload: { session_token: AUTH_SESSION_TOKEN } },
      { type: 'start_match', payload: {} },
    ]);
  });

  it('registers with invite code and then enters the table view', async () => {
    const user = userEvent.setup();
    const fetchMock = createFetchMock();
    vi.stubGlobal('fetch', fetchMock);

    render(<App />);

    await user.click(screen.getByRole('tab', { name: '邀请码注册' }));
    await user.type(screen.getByLabelText('邀请码'), 'INVITE-1');
    await user.type(screen.getByLabelText('昵称'), '新朋友');
    await user.type(screen.getByLabelText('密码'), 'secret-123');
    await user.click(screen.getByRole('button', { name: '注册并登录' }));

    await screen.findByRole('heading', { name: DEFAULT_CURRENT_USER.display_label });

    const registerCall = findFetchCall(fetchMock, '/api/auth/register', 'POST');
    expect(registerCall).toBeDefined();
    expect(parseRequestBody(registerCall?.[1])).toEqual({
      invite_code: 'INVITE-1',
      display_name: '新朋友',
      password: 'secret-123',
    });
  });

  it('shows the logged-in table view and uses the selected multiplier when creating a table', async () => {
    const user = userEvent.setup();
    const { fetchMock } = await renderAuthenticatedLobby();

    expect(screen.getByRole('heading', { name: DEFAULT_CURRENT_USER.display_label })).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'x3' }));
    await user.click(screen.getByRole('button', { name: '创建牌局' }));

    const createTableCall = findFetchCall(fetchMock, '/api/tables', 'POST');
    expect(createTableCall).toBeDefined();
    expect(parseRequestBody(createTableCall?.[1])).toEqual({ multiplier: 3 });

    await waitFor(() => {
      expect(getRoomSocket()).toBeDefined();
    });
  });

  it('calls the invite API from the sidebar', async () => {
    const user = userEvent.setup();
    const { fetchMock } = await renderAuthenticatedLobby();

    await user.click(screen.getByRole('button', { name: '创建牌局' }));

    const meSocket = getMeSocket();
    expect(meSocket).toBeDefined();
    const roomSocket = getRoomSocket();
    expect(roomSocket).toBeDefined();
    await act(async () => {
      roomSocket!.triggerOpen();
      roomSocket!.triggerMessage({
        type: 'room_snapshot',
        payload: {
          table_code: 'AB12CD',
          phase: 'waiting',
          seats: [],
          spectators: [],
          local_seat: 0,
          reconnect_token: 'token-1',
          match_state: null,
          private_state: null,
          owner_user_id: 1,
        },
      });
    });

    await act(async () => {
      meSocket!.triggerMessage({
        type: 'user_presence_updated',
        payload: {
          online_user_ids: [1, 2],
        },
      });
    });

    await user.click(screen.getAllByRole('button', { name: '邀请' })[0]!);

    const inviteCall = findFetchCall(fetchMock, '/api/tables/AB12CD/invites', 'POST');
    expect(inviteCall).toBeDefined();
    expect(parseRequestBody(inviteCall?.[1])).toEqual({
      invitee_user_id: 2,
    });
  });

  it('shows the invitation notice when /ws/me receives a new invite', async () => {
    await renderAuthenticatedLobby();

    const meSocket = getMeSocket();
    expect(meSocket).toBeDefined();

    await act(async () => {
      meSocket!.triggerMessage({
        type: 'table_invite_created',
        payload: DEFAULT_PENDING_INVITE,
      });
    });

    expect(screen.getByRole('region', { name: '牌局邀请' })).toBeInTheDocument();
    expect(screen.getByText('牌桌 ZXCVBN 邀请你加入。')).toBeInTheDocument();
  });

  it('removes a stale invite when accepting it reports the table no longer exists', async () => {
    const user = userEvent.setup();

    await renderAuthenticatedLobby({
      invites: [DEFAULT_PENDING_INVITE],
      acceptInviteStatus: 404,
      acceptInviteDetail: 'table_not_found',
    });

    expect(screen.getByText('ZXCVBN')).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: '接受' }));

    await waitFor(() => {
      expect(screen.getByText('牌桌不存在或已关闭。')).toBeInTheDocument();
    });
    expect(screen.queryByRole('region', { name: '牌局邀请' })).not.toBeInTheDocument();
    expect(screen.queryByText('ZXCVBN')).not.toBeInTheDocument();
    expect(screen.getByText('暂无待处理邀请')).toBeInTheDocument();
  });

  it('shows the aspect-ratio prompt for mobile portrait battle sessions', async () => {
    const user = userEvent.setup();
    mockMobileBattleImmersiveApis();

    const { socket } = await joinTable(user);

    await act(async () => {
      socket.triggerMessage({
        type: 'room_snapshot',
        payload: createPlayingSnapshotPayload(),
      });
    });

    expect(screen.getByText('请旋转屏幕或调整窗口比例')).toBeInTheDocument();
  });

  it('does not request fullscreen when a mobile user enters the battle screen or leave it', async () => {
    const user = userEvent.setup();
    const { requestFullscreen, exitFullscreen } = mockMobileBattleImmersiveApis();
    const { socket } = await joinTable(user);

    await act(async () => {
      socket.triggerMessage({
        type: 'room_snapshot',
        payload: createPlayingSnapshotPayload(),
      });
    });

    expect(requestFullscreen).not.toHaveBeenCalled();
    expect(screen.getByText('请旋转屏幕或调整窗口比例')).toBeInTheDocument();

    vi.spyOn(window, 'confirm').mockReturnValue(true);
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

    expect(exitFullscreen).not.toHaveBeenCalled();
  });

  it('keeps mobile battle sessions out of forced landscape retries', async () => {
    const user = userEvent.setup();
    const { lock } = mockMobileBattleImmersiveApis();
    const { socket } = await joinTable(user);

    expect(lock).not.toHaveBeenCalled();

    await act(async () => {
      socket.triggerMessage({
        type: 'room_snapshot',
        payload: createPlayingSnapshotPayload(),
      });
    });

    expect(lock).not.toHaveBeenCalled();
  });




  it('still asks for confirmation after the match has started', async () => {
    const user = userEvent.setup();
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(true);

    const { socket } = await joinTable(user);

    await act(async () => {
      socket.triggerMessage({
        type: 'room_snapshot',
        payload: createPlayingSnapshotPayload(),
      });
    });

    await user.click(await screen.findByRole('button', { name: '快捷离开牌桌' }));

    expect(confirmSpy).toHaveBeenCalledWith('若主动离开，则无法再次加入对局，是否确定离开牌桌？');
    expect(socket.sentMessages.map((message) => JSON.parse(message))).toEqual([
      { type: 'join_table', payload: { session_token: AUTH_SESSION_TOKEN } },
      { type: 'leave_table', payload: {} },
    ]);
  });

  it('returns to the table home as soon as leave_table_accepted arrives', async () => {
    const user = userEvent.setup();
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    const { socket } = await joinTable(user);

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

    expectTableHome();
  });

  it('returns to the table home with guidance when leaving after the connection has already dropped', async () => {
    const user = userEvent.setup();
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    const { socket } = await joinTable(user);

    await act(async () => {
      socket.triggerMessage({
        type: 'room_snapshot',
        payload: createPlayingSnapshotPayload(),
      });
    });

    await act(async () => {
      socket.close();
    });

    await user.click(await screen.findByRole('button', { name: '快捷离开牌桌' }));

    expectTableHome();
    expect(screen.getByText('当前连接已断开，已回到牌桌界面。若仍需回到牌局，请等待房主重新邀请。')).toBeInTheDocument();
  });

  it('returns to the table home when a stale room snapshot receives table_not_found', async () => {
    const user = userEvent.setup();
    const { socket } = await joinTable(user);

    await act(async () => {
      socket.triggerMessage({
        type: 'room_snapshot',
        payload: createPlayingSnapshotPayload(),
      });
    });

    await act(async () => {
      socket.triggerMessage({
        type: 'action_rejected',
        payload: {
          reason: 'table_not_found',
        },
      });
    });

    expectTableHome();
    expect(screen.getByText('牌桌不存在或已关闭。')).toBeInTheDocument();
  });

  it('stops retrying cached reconnects after repeated socket closes', async () => {
    vi.useFakeTimers();

    try {
      const fetchMock = createFetchMock();
      vi.stubGlobal('fetch', fetchMock);
      seedStoredAuthSession();
      localStorage.setItem(
        'mahjong:session',
        JSON.stringify({
          tableCode: 'AB12CD',
          nickname: 'Player A',
          reconnectToken: 'token-1',
          wsBaseUrl: 'ws://localhost:8000',
        }),
      );

      render(<App />);
      await act(async () => {
        await Promise.resolve();
      });
      await act(async () => {
        await Promise.resolve();
      });
      expect(screen.getByRole('heading', { name: DEFAULT_CURRENT_USER.display_label })).toBeInTheDocument();

      for (let attempt = 0; attempt < 3; attempt += 1) {
        await act(async () => {
          vi.advanceTimersByTime(1000);
        });

        const socket = getRoomSockets()[attempt];
        expect(socket).toBeDefined();

        await act(async () => {
          socket!.close();
        });
      }

      await act(async () => {
        vi.advanceTimersByTime(1000);
      });

      expect(getRoomSockets()).toHaveLength(3);
      expectTableHome();
      expect(screen.getByText('未能恢复座位，请回到牌桌侧栏后重新进入可加入的牌局。')).toBeInTheDocument();
    } finally {
      vi.useRealTimers();
    }
  });

  it('clears preselected claim tiles after passing', async () => {
    const user = userEvent.setup();
    const { socket } = await joinTable(user);

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
      { type: 'join_table', payload: { session_token: AUTH_SESSION_TOKEN } },
      { type: 'action_request', payload: { action_type: 'pass', tile_ids: [] } },
    ]);
  });

  it('sends pass for a local self-hu prompt so the server can advance the turn', async () => {
    const user = userEvent.setup();
    const { socket } = await joinTable(user);
    const baseSnapshot = createPlayingSnapshotPayload();

    await act(async () => {
      socket.triggerMessage({
        type: 'room_snapshot',
        payload: createPlayingSnapshotPayload({
          private_state: {
            ...baseSnapshot.private_state,
            pending_action: {
              type: 'active_turn',
              seat_index: 0,
              deadline_at: '2026-03-27T12:00:00Z',
              drawn_tile_id: 'w1#1',
              options: ['discard', 'hu', 'pass'],
            },
          },
        }),
      });
    });

    expect(screen.getByRole('button', { name: '过' })).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: '过' }));

    expect(screen.queryByRole('button', { name: '过' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '和牌' })).not.toBeInTheDocument();
    expect(socket.sentMessages.map((message) => JSON.parse(message))).toEqual([
      { type: 'join_table', payload: { session_token: AUTH_SESSION_TOKEN } },
      { type: 'action_request', payload: { action_type: 'pass', tile_ids: [] } },
    ]);
  });

  it('sends a flower action directly from the local active turn', async () => {
    const user = userEvent.setup();
    const { socket } = await joinTable(user);
    const baseSnapshot = createPlayingSnapshotPayload();

    await act(async () => {
      socket.triggerMessage({
        type: 'room_snapshot',
        payload: createPlayingSnapshotPayload({
          private_state: {
            ...baseSnapshot.private_state,
            pending_action: {
              type: 'active_turn',
              seat_index: 0,
              deadline_at: '2026-03-27T12:00:00Z',
              drawn_tile_id: 'f1#0',
              options: ['discard', 'flower'],
            },
            players: [
              {
                ...baseSnapshot.private_state.players[0],
                concealed_count: 2,
                concealed_tiles: [
                  { tile_id: 'f1#0', tile_key: 'f1' },
                  { tile_id: 'w2#2', tile_key: 'w2' },
                ],
              },
              ...baseSnapshot.private_state.players.slice(1),
            ],
          },
        }),
      });
    });

    await user.click(screen.getByRole('button', { name: '春' }));
    expect(screen.getByRole('button', { name: '补花' })).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: '补花' }));

    await waitFor(() => {
      expect(socket.sentMessages.map((message) => JSON.parse(message))).toEqual([
        { type: 'join_table', payload: { session_token: AUTH_SESSION_TOKEN } },
        { type: 'action_request', payload: { action_type: 'flower', tile_ids: ['f1#0'] } },
      ]);
    });
  });

  it('sends ready_hand with the selected tile and immediately shows the ting callout instead of a normal discard spotlight', async () => {
    const user = userEvent.setup();
    const { socket } = await joinTable(user);
    const selectedTileId = 'b9#0';
    const baseSnapshot = createPlayingSnapshotPayload();

    await act(async () => {
      socket.triggerMessage({
        type: 'room_snapshot',
        payload: createPlayingSnapshotPayload({
          private_state: {
            ...baseSnapshot.private_state,
            pending_action: {
              type: 'active_turn',
              seat_index: 0,
              deadline_at: '2026-03-27T12:00:00Z',
              drawn_tile_id: selectedTileId,
              options: ['discard', 'ready_hand'],
            },
            hand_insights: {
              current: null,
              by_discard_tile_id: {
                [selectedTileId]: {
                  discard_tile_id: selectedTileId,
                  discard_tile_code: 'b9',
                  is_tenpai: true,
                  waits: [{ code: 't4', available_count: 3 }],
                  winning_fans: [{ fan_key: 'full_flush', fan_value: 24 }],
                },
              },
            },
            players: [
              {
                ...baseSnapshot.private_state.players[0],
                concealed_count: 14,
                concealed_tiles: [
                  { tile_id: 'w1#0', tile_key: 'w1' },
                  { tile_id: 'w2#0', tile_key: 'w2' },
                  { tile_id: 'w3#0', tile_key: 'w3' },
                  { tile_id: 'w4#0', tile_key: 'w4' },
                  { tile_id: 'w5#0', tile_key: 'w5' },
                  { tile_id: 'w6#0', tile_key: 'w6' },
                  { tile_id: 'w7#0', tile_key: 'w7' },
                  { tile_id: 'w8#0', tile_key: 'w8' },
                  { tile_id: 'w9#0', tile_key: 'w9' },
                  { tile_id: 't1#0', tile_key: 't1' },
                  { tile_id: 't2#0', tile_key: 't2' },
                  { tile_id: 't3#0', tile_key: 't3' },
                  { tile_id: 't4#0', tile_key: 't4' },
                  { tile_id: selectedTileId, tile_key: 'b9' },
                ],
              },
              ...baseSnapshot.private_state.players.slice(1),
            ],
          },
        }),
      });
    });

    await user.click(getLocalHandButtons().at(-1)!);
    expect(screen.getByRole('button', { name: '听' })).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: '听' }));

    await waitFor(() => {
      expect(getLocalHandButtons()).toHaveLength(13);
      expect(screen.getByText('听')).toBeInTheDocument();
      expect(screen.queryByLabelText('Latest discard spotlight')).toBeNull();
      expect(socket.sentMessages.map((message) => JSON.parse(message))).toEqual([
        { type: 'join_table', payload: { session_token: AUTH_SESSION_TOKEN } },
        { type: 'action_request', payload: { action_type: 'ready_hand', tile_ids: [selectedTileId] } },
      ]);
    });
  });

  it('clears preselected claim tiles after the claim window times out and play resumes', async () => {
    const user = userEvent.setup();
    const { socket } = await joinTable(user);

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
    const { socket } = await joinTable(user);

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

  it('highlights matching non-hand tiles after selecting a hand tile', async () => {
    const user = userEvent.setup();
    const { socket } = await joinTable(user);

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
              drawn_tile_id: 'b3#2',
              options: ['discard'],
            },
            players: [
              {
                seat_index: 0,
                nickname: 'Player A',
                connected: true,
                concealed_count: 3,
                concealed_tiles: [
                  { tile_id: 'w2#0', tile_key: 'w2' },
                  { tile_id: 'w2#1', tile_key: 'w2' },
                  { tile_id: 'b3#2', tile_key: 'b3' },
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
                melds: [['w2', 'w2', 'w2']],
                flowers: [],
                discards: ['w2'],
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

    expect(countRelatedHighlightTiles(document.body)).toBe(0);

    await user.click(getLocalHandButtons()[0]!);

    expect(countSelectedTiles(document.body)).toBe(1);
    expect(countRelatedHighlightTiles(document.body)).toBe(4);
  });

  it('lets the player choose a claim candidate pane before confirming chow', async () => {
    const user = userEvent.setup();
    const { socket } = await joinTable(user);

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
      { type: 'join_table', payload: { session_token: AUTH_SESSION_TOKEN } },
      { type: 'action_request', payload: { action_type: 'chow', tile_ids: ['w1#1', 'w2#2'] } },
    ]);
  });

  it('submits the default selected chow pair when the chow button is clicked directly', async () => {
    const user = userEvent.setup();
    const { socket } = await joinTable(user);

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
      { type: 'join_table', payload: { session_token: AUTH_SESSION_TOKEN } },
      { type: 'action_request', payload: { action_type: 'chow', tile_ids: ['w1#1', 'w2#2'] } },
    ]);
  });

  it('supports double-clicking a hand tile to discard immediately', async () => {
    const user = userEvent.setup();
    const { socket } = await joinTable(user);

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

    expect(getLocalHandButtons()).toHaveLength(1);
    expect(screen.getByLabelText('Latest discard spotlight')).toBeInTheDocument();
    expect(countSelectedTiles(document.body)).toBe(0);
    expect(socket.sentMessages.map((message) => JSON.parse(message))).toEqual([
      { type: 'join_table', payload: { session_token: AUTH_SESSION_TOKEN } },
      { type: 'action_request', payload: { action_type: 'discard', tile_ids: ['w2#2'] } },
    ]);
  });

  it('rolls back an optimistic discard if the server rejects it', async () => {
    const user = userEvent.setup();
    const { socket } = await joinTable(user);

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

    await user.dblClick(getLocalHandButtons()[1]!);
    expect(getLocalHandButtons()).toHaveLength(1);
    expect(screen.getByLabelText('Latest discard spotlight')).toBeInTheDocument();

    await act(async () => {
      socket.triggerMessage({
        type: 'action_rejected',
        payload: {
          reason: 'invalid_action',
        },
      });
    });

    expect(getLocalHandButtons()).toHaveLength(2);
    expect(screen.queryByLabelText('Latest discard spotlight')).toBeNull();
  });


  it('still allows sending kong from the local kong-response prompt', async () => {
    const user = userEvent.setup();
    const { socket } = await joinTable(user);

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
      { type: 'join_table', payload: { session_token: AUTH_SESSION_TOKEN } },
      { type: 'action_request', payload: { action_type: 'kong', tile_ids: ['w3#0', 'w3#1', 'w3#2', 'w3#3'] } },
    ]);
  });

  it('still allows a ready-hand player to claim kong from the claim window', async () => {
    const user = userEvent.setup();
    const { socket } = await joinTable(user);

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
              options: ['kong'],
            },
            players: [
              {
                seat_index: 0,
                nickname: 'Player A',
                connected: true,
                is_ready_hand: true,
                concealed_count: 13,
                concealed_tiles: [
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

    expect(countSelectedTiles(document.body)).toBe(3);

    await user.click(screen.getByRole('button', { name: '杠' }));

    expect(socket.sentMessages.map((message) => JSON.parse(message))).toEqual([
      { type: 'join_table', payload: { session_token: AUTH_SESSION_TOKEN } },
      { type: 'action_request', payload: { action_type: 'kong', tile_ids: ['w3#1', 'w3#2', 'w3#3'] } },
    ]);
  });

  it('submits the matching claim action immediately when a candidate pane is double-clicked', async () => {
    const user = userEvent.setup();
    const { socket } = await joinTable(user);

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
      { type: 'join_table', payload: { session_token: AUTH_SESSION_TOKEN } },
      { type: 'action_request', payload: { action_type: 'chow', tile_ids: ['w1#1', 'w2#2'] } },
    ]);
  });



  it('renders a barrage line when a quick-chat broadcast arrives from the server', async () => {
    const user = userEvent.setup();
    const { socket } = await joinTable(user);

    await act(async () => {
      socket.triggerMessage({
        type: 'room_snapshot',
        payload: createPlayingSnapshotPayload(),
      });
    });

    await act(async () => {
      socket.triggerMessage({
        type: 'quick_chat',
        payload: {
          message_id: 'quick-chat-1',
          actor_seat: 0,
          target_seat: 1,
          emoji: '🀄',
          sent_at: '2026-04-02T02:00:00Z',
        },
      });
    });

    expect(screen.getByText('Player A -> Player B : 🀄')).toBeInTheDocument();
  });
});
