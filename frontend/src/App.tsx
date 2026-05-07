import { useEffect, useEffectEvent, useMemo, useReducer, useRef, useState, type MutableRefObject } from 'react';

import { AuthGate } from './components/auth/AuthGate';
import { BattleScreen } from './components/battle-screen/BattleScreen';
import type { TableSidebarPlayer, TableSidebarSpectator } from './components/table-sidebar/TableSidebar';
import { SocialSidebarPanel } from './components/lobby/SocialSidebarPanel';
import {
  clearStoredAuthSession,
  getMe,
  loadStoredAuthSession,
  loginWithPassword,
  logoutSession,
  registerWithInvite,
  saveStoredAuthSession,
} from './lib/authApi';
import { useSequentialBackgroundMusic } from './lib/backgroundMusic';
import {
  getActionCandidateGroups,
  getFlowerCandidateTileIds,
  getLocalTurnKongCandidateGroups,
  getLocalTurnKongPromptSignature,
  getMatchingActionGroup,
} from './lib/kongSelection';
import { createClaimCandidates, createMatchViewModel, getLocalSelfHuPromptSignature } from './lib/matchViewModel';
import {
  buildWebSocketUrl,
  createAdjustBotsMessage,
  createActionRequestMessage,
  createHeartbeatMessage,
  createJoinTableMessage,
  createLeaveTableMessage,
  createQuickChatMessage,
  createReadyMessage,
  createReconnectMessage,
  createRestartMatchMessage,
  createSetBotTakeoverMessage,
  createStartMatchMessage,
  createStartNextRoundMessage,
  createWatchTableMessage,
  parseServerMessage,
  serializeClientMessage,
} from './lib/socket';
import { buildMeSocketUrl, parseSocialServerMessage } from './lib/meSocket';
import {
  acceptTableInvite,
  approveSpectatorRequest,
  createSocialTable,
  createTableInvite,
  getLeaderboard,
  getMyInvites,
  getMySpectatorRequests,
  getUserFans,
  getUserGames,
  rejectSpectatorRequest,
} from './lib/socialApi';
import { createInitialSessionState, sessionReducer } from './lib/sessionReducer';
import {
  clearStoredSession,
  loadStoredSession,
  loadStoredThemeId,
  saveStoredSession,
  saveStoredThemeId,
  loadStoredBgmEnabled,
  saveStoredBgmEnabled,
} from './lib/storage';
import { DEFAULT_THEME_ID, getNextThemeId, getRandomThemeId, getThemeLabel, isThemeId } from './lib/themes';
import type {
  BackendActionType,
  BattleActionId,
  ClaimActionId,
  ClientMode,
  GameSummary,
  PublicUser,
  QuickChatEmoji,
  SessionState,
  SpectatorRequest,
  TableInvite,
  UserFanStat,
} from './types/match';

const HEARTBEAT_INTERVAL_MS = 20_000;
const MAX_CACHED_RECONNECT_CLOSES = 3;
const LEAVE_TABLE_CONFIRM_MESSAGE = '若主动离开，则无法再次加入对局，是否确定离开牌桌？';
const CLAIM_ACTION_IDS = ['chow', 'pung', 'kong'] as const;
const BOT_TAKEOVER_ROOM_ACTION_IDS = new Set<BattleActionId>([
  'ready',
  'start_match',
  'start_next_round',
  'restart_match',
]);
type AuthStatus = 'loading' | 'anonymous' | 'ready';

function getRuntimeDefaultBaseUrls() {
  if (typeof window === 'undefined') {
    return {
      apiBaseUrl: 'http://localhost:8000',
      wsBaseUrl: 'ws://localhost:8000',
    };
  }

  const { origin, protocol, host } = window.location;
  return {
    apiBaseUrl: origin,
    wsBaseUrl: `${protocol === 'https:' ? 'wss' : 'ws'}://${host}`,
  };
}

function getDefaultConfig() {
  const env = ((import.meta as ImportMeta & { env?: Record<string, string | undefined> }).env ?? {});
  const runtimeDefaults = getRuntimeDefaultBaseUrls();
  const defaults = {
    apiBaseUrl: env.VITE_API_BASE_URL ?? runtimeDefaults.apiBaseUrl,
    wsBaseUrl: env.VITE_WS_BASE_URL ?? runtimeDefaults.wsBaseUrl,
  };
  const storedRoomSession = loadStoredSession();
  const storedAuthSession = loadStoredAuthSession();

  return {
    defaults,
    storedAuthSession,
    storedRoomSession,
  };
}

function getRejectedMessage(reason: string) {
  const lookup: Record<string, string> = {
    table_not_found: '牌桌不存在或已关闭。',
    table_full: '牌桌已经坐满了，请换一个牌桌试试。',
    invalid_reconnect_token: '上次的重连凭证已失效，请回到牌桌侧栏后重新进入可加入的牌局。',
    seat_occupied: '这个座位已经被占用，请选择其他空位。',
  };

  return lookup[reason] ?? '请求未被服务器接受，请按最新房间状态重试。';
}

function getSocialStatusCopy(detail: string) {
  const lookup: Record<string, string> = {
    auth_required: '登录状态已失效，请重新登录。',
    invite_code_invalid: '邀请码无效或已被使用。',
    invalid_credentials: '账号或密码错误。',
    username_taken: '该账号名已被占用。',
    target_player_busy: '该玩家正在牌局中，请稍后重试。',
    target_already_in_table: '该玩家已在本牌局中。',
    only_owner_can_invite: '只有房主可以邀请玩家。',
    table_multiplier_locked: '牌局已开始，无法再修改或发起等待区邀请。',
    table_not_found: '牌桌不存在或已关闭。',
    spectator_requires_owner_approval: '观战需要房主同意。',
    player_cannot_watch_own_table: '牌局内玩家不能申请观战本局。',
    spectator_request_not_found: '观战申请不存在或已处理。',
  };

  return lookup[detail] ?? detail;
}

function closeSocket(socketRef: MutableRefObject<WebSocket | null>, heartbeatTimerRef: MutableRefObject<number | null>) {
  if (heartbeatTimerRef.current !== null) {
    window.clearInterval(heartbeatTimerRef.current);
    heartbeatTimerRef.current = null;
  }

  if (!socketRef.current) {
    return;
  }

  socketRef.current.onclose = null;
  socketRef.current.close();
  socketRef.current = null;
}

function hasClaimAction(options: BackendActionType[]) {
  return CLAIM_ACTION_IDS.some((actionId) => options.includes(actionId));
}

function getClaimSelectionSignature(state: SessionState) {
  const pendingAction = state.roomSnapshot?.payload.private_state?.pending_action;

  if (pendingAction?.type === 'claim_window' && Array.isArray(pendingAction.options)) {
    const options = pendingAction.options
      .filter((option): option is BackendActionType => typeof option === 'string')
      .filter((option): option is ClaimActionId => CLAIM_ACTION_IDS.includes(option as ClaimActionId));
    return options.length > 0 ? `claim:${pendingAction.deadline_at}:${options.slice().sort().join(',')}` : null;
  }

  const promptOptions = (state.latestActionPrompt?.payload.options ?? []).filter(
    (option): option is BackendActionType => CLAIM_ACTION_IDS.includes(option as ClaimActionId) || option === 'pass',
  );
  if (promptOptions.includes('pass') && hasClaimAction(promptOptions)) {
    const options = promptOptions.filter((option): option is ClaimActionId => CLAIM_ACTION_IDS.includes(option as ClaimActionId));
    return options.length > 0 ? `claim:${state.latestActionPrompt?.payload.deadline_at ?? ''}:${options.slice().sort().join(',')}` : null;
  }

  return null;
}

function canUseClaimMultiSelect(state: SessionState) {
  return getClaimSelectionSignature(state) !== null;
}

function getDefaultClaimCandidateSelection(state: SessionState) {
  const firstCandidate = createClaimCandidates(state)[0];

  if (!firstCandidate) {
    return null;
  }

  return {
    actionId: firstCandidate.actionId,
    tileIds: firstCandidate.tileIds,
  };
}

function canQuickDiscard(state: SessionState, hasLocalTurnKongPrompt: boolean) {
  if (state.optimisticDiscard) {
    return false;
  }

  if (
    hasLocalTurnKongPrompt ||
    canUseClaimMultiSelect(state) ||
    state.selectionMode === 'kong' ||
    state.selectionMode === 'chow' ||
    state.selectionMode === 'pung'
  ) {
    return false;
  }

  const localSeat = state.roomSnapshot?.payload.local_seat;
  if (typeof localSeat !== 'number') {
    return false;
  }

  const pendingAction = state.roomSnapshot?.payload.private_state?.pending_action;
  if (
    pendingAction?.type === 'active_turn' &&
    pendingAction.seat_index === localSeat &&
    Array.isArray(pendingAction.options) &&
    pendingAction.options.includes('discard')
  ) {
    return true;
  }

  return state.latestActionPrompt?.payload.seat_index === localSeat && state.latestActionPrompt.payload.options.includes('discard');
}

function getOccupiedSpectatorSeats(snapshot: SessionState['roomSnapshot']) {
  return snapshot?.payload.seats.map((seat) => seat.seat_index).sort((left, right) => left - right) ?? [];
}

function resolveSpectatorFocusSeat(state: SessionState) {
  const seats = getOccupiedSpectatorSeats(state.roomSnapshot);
  if (seats.length === 0) {
    return 0;
  }

  if (typeof state.spectatorFocusSeat === 'number' && seats.includes(state.spectatorFocusSeat)) {
    return state.spectatorFocusSeat;
  }

  return seats.includes(0) ? 0 : seats[0];
}

function isActionBlockedByOptimisticDiscard(actionId: BattleActionId) {
  return (
    actionId === 'discard' ||
    actionId === 'ready_hand' ||
    actionId === 'flower' ||
    actionId === 'kong' ||
    actionId === 'hu' ||
    actionId === 'chow' ||
    actionId === 'pung' ||
    actionId === 'pass'
  );
}

function upsertInvite(current: TableInvite[], nextInvite: TableInvite) {
  if (!isPendingTableInvite(nextInvite)) {
    return removeInviteById(current, nextInvite.id);
  }

  return [nextInvite, ...current.filter((invite) => invite.id !== nextInvite.id)];
}

function removeInviteById(current: TableInvite[], inviteId: number) {
  return current.filter((invite) => invite.id !== inviteId);
}

function isPendingTableInvite(invite: TableInvite) {
  return invite.status === 'pending';
}

function getPendingTableInvites(invites: TableInvite[]) {
  return invites.filter(isPendingTableInvite);
}

function isStaleTableInviteError(error: unknown) {
  return error instanceof Error && error.message === 'table_not_found';
}

function upsertSpectatorRequest(current: SpectatorRequest[], nextRequest: SpectatorRequest) {
  return [nextRequest, ...current.filter((request) => request.id !== nextRequest.id)];
}

function removeSpectatorRequestById(current: SpectatorRequest[], requestId: number) {
  return current.filter((request) => request.id !== requestId);
}

function getSeatLabel(seatIndex?: number | null) {
  const windLabels = ['东位', '南位', '西位', '北位'];
  if (typeof seatIndex !== 'number' || seatIndex < 0) {
    return '未知座位';
  }

  return windLabels[seatIndex] ?? `${seatIndex + 1}号位`;
}

export default function App() {
  const [isBgmEnabled, setIsBgmEnabled] = useState(() => loadStoredBgmEnabled());
  const [isVoiceEnabled, setIsVoiceEnabled] = useState(true);
  const { defaults, storedAuthSession, storedRoomSession } = useMemo(getDefaultConfig, []);
  const [themeId, setThemeId] = useState(() => {
    const storedThemeId = loadStoredThemeId();
    const nextThemeId = isThemeId(storedThemeId) ? getRandomThemeId(storedThemeId) : getRandomThemeId();

    return isThemeId(nextThemeId) ? nextThemeId : DEFAULT_THEME_ID;
  });
  const [statusMessage, setStatusMessage] = useState<string | null>(
    storedRoomSession ? `检测到牌桌 ${storedRoomSession.tableCode} 的历史会话，正在尝试恢复座位。` : null,
  );
  const [authStatus, setAuthStatus] = useState<AuthStatus>(storedAuthSession ? 'loading' : 'anonymous');
  const [authSession, setAuthSession] = useState(storedAuthSession);
  const [currentUser, setCurrentUser] = useState<PublicUser | null>(storedAuthSession?.user ?? null);
  const [leaderboard, setLeaderboard] = useState<PublicUser[]>([]);
  const [onlineUserIds, setOnlineUserIds] = useState<number[]>([]);
  const [pendingInvites, setPendingInvites] = useState<TableInvite[]>([]);
  const [pendingSpectatorRequests, setPendingSpectatorRequests] = useState<SpectatorRequest[]>([]);
  const [inviteDialog, setInviteDialog] = useState<TableInvite | null>(null);
  const [activeLobbyTableCode, setActiveLobbyTableCode] = useState<string | null>(null);
  const [currentTableOwnerUserId, setCurrentTableOwnerUserId] = useState<number | null>(null);
  const [selectedProfileUser, setSelectedProfileUser] = useState<PublicUser | null>(storedAuthSession?.user ?? null);
  const [selectedProfileFallbackName, setSelectedProfileFallbackName] = useState<string | null>(
    storedAuthSession?.user.display_name ?? null,
  );
  const [profileFanStats, setProfileFanStats] = useState<UserFanStat[]>([]);
  const [profileRecentGames, setProfileRecentGames] = useState<GameSummary[]>([]);
  const [profileLoading, setProfileLoading] = useState(false);
  const [profileMessage, setProfileMessage] = useState<string | null>(null);
  const [state, dispatch] = useReducer(
    sessionReducer,
    undefined,
    (): SessionState => ({
      ...createInitialSessionState(),
      apiBaseUrl: defaults.apiBaseUrl,
      wsBaseUrl: storedRoomSession?.wsBaseUrl ?? defaults.wsBaseUrl,
      tableCode: storedRoomSession?.tableCode ?? '',
      nickname: storedAuthSession?.user.display_name ?? storedRoomSession?.nickname ?? '',
      reconnectToken: storedRoomSession?.reconnectToken ?? null,
      connectionStatus: 'idle',
    }),
  );
  const socketRef = useRef<WebSocket | null>(null);
  const meSocketRef = useRef<WebSocket | null>(null);
  const heartbeatTimerRef = useRef<number | null>(null);
  const sessionRef = useRef(state);
  const leavingTableRef = useRef(false);
  const reconnectCloseCountRef = useRef(0);
  const previousClaimSelectionSignatureRef = useRef<string | null>(null);
  const previousLocalTurnKongPromptSignatureRef = useRef<string | null>(null);
  const previousHadRoomSnapshotRef = useRef(false);
  const [dismissedLocalTurnKongPromptSignature, setDismissedLocalTurnKongPromptSignature] = useState<string | null>(null);
  const [dismissedLocalSelfHuPromptSignature, setDismissedLocalSelfHuPromptSignature] = useState<string | null>(null);

  useEffect(() => {
    sessionRef.current = state;
  }, [state]);

  useEffect(() => {
    if (typeof document === 'undefined') {
      return;
    }

    document.documentElement.dataset.theme = themeId;
    saveStoredThemeId(themeId);
  }, [themeId]);

  useEffect(() => {
    if (!state.roomSnapshot && previousHadRoomSnapshotRef.current) {
      setThemeId((currentThemeId) => getRandomThemeId(currentThemeId));
    }

    previousHadRoomSnapshotRef.current = state.roomSnapshot !== null;
  }, [state.roomSnapshot]);

  useEffect(() => {
    let cancelled = false;

    async function bootstrapAuth() {
      if (!authSession?.sessionToken) {
        setAuthStatus('anonymous');
        setCurrentUser(null);
        setLeaderboard([]);
        setOnlineUserIds([]);
        setPendingInvites([]);
        setPendingSpectatorRequests([]);
        setInviteDialog(null);
        setSelectedProfileUser(null);
        setSelectedProfileFallbackName(null);
        setProfileFanStats([]);
        setProfileRecentGames([]);
        setProfileMessage(null);
        setCurrentTableOwnerUserId(null);
        if (meSocketRef.current) {
          meSocketRef.current.onclose = null;
          meSocketRef.current.close();
          meSocketRef.current = null;
        }
        return;
      }

      setAuthStatus('loading');

      try {
        const [me, nextInvites, nextLeaderboard, nextSpectatorRequests] = await Promise.all([
          getMe(defaults.apiBaseUrl, authSession.sessionToken),
          getMyInvites(defaults.apiBaseUrl, authSession.sessionToken),
          getLeaderboard(defaults.apiBaseUrl),
          getMySpectatorRequests(defaults.apiBaseUrl, authSession.sessionToken),
        ]);

        if (cancelled) {
          return;
        }

        setCurrentUser(me);
        setPendingInvites(getPendingTableInvites(nextInvites));
        setPendingSpectatorRequests(nextSpectatorRequests);
        setLeaderboard(nextLeaderboard);
        setSelectedProfileUser((current) => current ?? me);
        setSelectedProfileFallbackName((current) => current ?? me.display_name);
        setAuthStatus('ready');
        setStatusMessage((current) =>
          current?.includes('历史会话') ? current : null,
        );
        saveStoredAuthSession({
          sessionToken: authSession.sessionToken,
          user: me,
        });
        setAuthSession((current) =>
          current && current.sessionToken === authSession.sessionToken
            ? {
                sessionToken: current.sessionToken,
                user: me,
              }
            : current,
        );
        dispatch({ type: 'set_credentials', nickname: me.display_name });
      } catch (error) {
        if (cancelled) {
          return;
        }

        clearStoredAuthSession();
        clearStoredSession();
        setAuthSession(null);
        setCurrentUser(null);
        setLeaderboard([]);
        setOnlineUserIds([]);
        setPendingInvites([]);
        setPendingSpectatorRequests([]);
        setInviteDialog(null);
        setSelectedProfileUser(null);
        setSelectedProfileFallbackName(null);
        setProfileFanStats([]);
        setProfileRecentGames([]);
        setAuthStatus('anonymous');
        setStatusMessage(error instanceof Error ? getSocialStatusCopy(error.message) : '登录状态已失效，请重新登录。');
      }
    }

    void bootstrapAuth();

    return () => {
      cancelled = true;
    };
  }, [authSession?.sessionToken, defaults.apiBaseUrl]);

  useEffect(() => {
    if (authStatus !== 'ready' || !authSession?.sessionToken || !state.wsBaseUrl) {
      return;
    }

    const socket = new WebSocket(buildMeSocketUrl(state.wsBaseUrl, authSession.sessionToken));
    meSocketRef.current = socket;

    socket.onmessage = (event) => {
      const message = parseSocialServerMessage(String(event.data));
      if (!message) {
        return;
      }

      if (message.type === 'user_presence_updated') {
        setOnlineUserIds(message.payload.online_user_ids);
        return;
      }

      if (message.type === 'table_invite_created') {
        setPendingInvites((current) => upsertInvite(current, message.payload));
        setInviteDialog((current) => {
          if (isPendingTableInvite(message.payload)) {
            return message.payload;
          }

          return current?.id === message.payload.id ? null : current;
        });
        return;
      }

      if (message.type === 'spectator_request_created') {
        setPendingSpectatorRequests((current) => upsertSpectatorRequest(current, message.payload));
        setStatusMessage(`收到牌桌 ${message.payload.table_code} 的观战申请。`);
        return;
      }

      if (message.type === 'spectator_request_decided') {
        setPendingSpectatorRequests((current) => removeSpectatorRequestById(current, message.payload.id));
        setStatusMessage(
          message.payload.status === 'approved'
            ? `牌桌 ${message.payload.table_code} 已允许观战。`
            : `牌桌 ${message.payload.table_code} 拒绝了观战申请。`,
        );
      }
    };

    socket.onclose = () => {
      if (meSocketRef.current === socket) {
        meSocketRef.current = null;
      }
    };

    return () => {
      socket.onclose = null;
      socket.close();
      if (meSocketRef.current === socket) {
        meSocketRef.current = null;
      }
    };
  }, [authSession?.sessionToken, authStatus, state.wsBaseUrl]);

  useEffect(() => {
    if (!selectedProfileUser) {
      setProfileFanStats([]);
      setProfileRecentGames([]);
      setProfileMessage(selectedProfileFallbackName ? '该玩家暂无可用公开账号数据。' : null);
      setProfileLoading(false);
      return;
    }

    let cancelled = false;
    setProfileLoading(true);
    setProfileMessage(null);

    void Promise.all([
      getUserFans(defaults.apiBaseUrl, selectedProfileUser.user_id),
      getUserGames(defaults.apiBaseUrl, selectedProfileUser.user_id),
    ])
      .then(([fans, games]) => {
        if (cancelled) {
          return;
        }

        setProfileFanStats(fans);
        setProfileRecentGames(games);
        setProfileLoading(false);
        setProfileMessage(null);
      })
      .catch((error) => {
        if (cancelled) {
          return;
        }

        setProfileFanStats([]);
        setProfileRecentGames([]);
        setProfileLoading(false);
        setProfileMessage(error instanceof Error ? getSocialStatusCopy(error.message) : '公开资料加载失败。');
      });

    return () => {
      cancelled = true;
    };
  }, [defaults.apiBaseUrl, selectedProfileFallbackName, selectedProfileUser]);

  useEffect(() => {
    if (
      authStatus !== 'ready' ||
      state.connectionStatus !== 'idle' ||
      !state.reconnectToken ||
      !state.tableCode ||
      state.roomSnapshot
    ) {
      return;
    }

    dispatch({ type: 'set_connection_status', status: 'reconnecting' });
  }, [authStatus, state.connectionStatus, state.reconnectToken, state.roomSnapshot, state.tableCode]);

  useEffect(() => {
    if (state.clientMode === 'spectator') {
      clearStoredSession();
      return;
    }

    if (state.reconnectToken && state.tableCode && state.wsBaseUrl) {
      saveStoredSession({
        tableCode: state.tableCode,
        nickname: currentUser?.display_name ?? state.nickname,
        reconnectToken: state.reconnectToken,
        wsBaseUrl: state.wsBaseUrl,
      });
      return;
    }

    clearStoredSession();
  }, [currentUser?.display_name, state.clientMode, state.nickname, state.reconnectToken, state.tableCode, state.wsBaseUrl]);

  const handleLeaveToLobby = useEffectEvent((tableCode?: string, nextStatusMessage: string | null = null) => {
    leavingTableRef.current = false;
    reconnectCloseCountRef.current = 0;
    setCurrentTableOwnerUserId(null);
    clearStoredSession();
    dispatch({
      type: 'return_to_lobby',
      tableCode: tableCode ?? sessionRef.current.tableCode,
    });
    closeSocket(socketRef, heartbeatTimerRef);
    setStatusMessage(nextStatusMessage);
  });

  const handleFatalLobbyReset = useEffectEvent((message: string, tableCode?: string) => {
    reconnectCloseCountRef.current = 0;
    setCurrentTableOwnerUserId(null);
    clearStoredSession();
    dispatch({
      type: 'return_to_lobby',
      tableCode: tableCode ?? sessionRef.current.tableCode,
    });
    closeSocket(socketRef, heartbeatTimerRef);
    setStatusMessage(message);
  });

  const handleServerMessage = useEffectEvent((raw: string) => {
    const message = parseServerMessage(raw);
    if (!message) {
      setStatusMessage('收到了一条无法识别的服务器消息。');
      return;
    }

    if (message.type === 'action_rejected') {
      const current = sessionRef.current;
      const isFatalTableMissing = message.payload.reason === 'table_not_found';
      const isFatalReconnectFailure = message.payload.reason === 'invalid_reconnect_token';
      const isFatalJoinFailure = !current.roomSnapshot && message.payload.reason === 'table_full';

      if (leavingTableRef.current) {
        leavingTableRef.current = false;
      }

      if (isFatalReconnectFailure || isFatalTableMissing || isFatalJoinFailure) {
        handleFatalLobbyReset(getRejectedMessage(message.payload.reason), current.tableCode);
        return;
      }
    }

    if (message.type === 'leave_table_accepted') {
      handleLeaveToLobby(message.payload.table_code);
      return;
    }

    if (message.type === 'room_snapshot') {
      reconnectCloseCountRef.current = 0;
      dispatch({ type: 'set_connection_status', status: 'connected' });
      setStatusMessage(null);
    }

    dispatch({ type: 'ws_message', message });
  });

  const openRoomSocket = useEffectEvent(
    (options: {
      tableCode: string;
      nickname: string;
      wsBaseUrl: string;
      sessionToken?: string | null;
      reconnectToken?: string | null;
      reconnect?: boolean;
      mode?: ClientMode;
    }) => {
      closeSocket(socketRef, heartbeatTimerRef);

      const { tableCode, nickname, wsBaseUrl, sessionToken, reconnectToken, reconnect, mode = 'player' } = options;
      if (!reconnect) {
        reconnectCloseCountRef.current = 0;
      }
      dispatch({ type: 'set_client_mode', clientMode: mode });
      dispatch({ type: 'set_connection_status', status: reconnect ? 'reconnecting' : 'connecting' });
      dispatch({ type: 'set_credentials', tableCode, nickname });
      dispatch({ type: 'set_config', wsBaseUrl });

      const socket = new WebSocket(buildWebSocketUrl(wsBaseUrl, tableCode));
      socketRef.current = socket;

      socket.onopen = () => {
        void (async () => {
          const message =
            mode === 'spectator'
              ? createWatchTableMessage(sessionToken ?? '', nickname)
              : reconnect && reconnectToken
                ? createReconnectMessage(reconnectToken)
                : createJoinTableMessage(sessionToken ?? '');
          socket.send(serializeClientMessage(message));

          heartbeatTimerRef.current = window.setInterval(() => {
            if (socket.readyState === WebSocket.OPEN) {
              socket.send(serializeClientMessage(createHeartbeatMessage(new Date().toISOString())));
            }
          }, HEARTBEAT_INTERVAL_MS);
        })();
      };

      socket.onmessage = (event) => {
        handleServerMessage(String(event.data));
      };

      socket.onerror = () => {
        setStatusMessage('通信连接失败。');
      };

      socket.onclose = () => {
        if (heartbeatTimerRef.current !== null) {
          window.clearInterval(heartbeatTimerRef.current);
          heartbeatTimerRef.current = null;
        }

        socketRef.current = null;
        const current = sessionRef.current;
        if (leavingTableRef.current) {
          handleLeaveToLobby(current.tableCode);
          return;
        }
        if (current.clientMode === 'spectator') {
          handleLeaveToLobby(current.tableCode, '观战连接已断开。');
          return;
        }
        if (current.reconnectToken && current.tableCode && current.wsBaseUrl) {
          reconnectCloseCountRef.current += 1;
          if (
            !current.roomSnapshot &&
            reconnectCloseCountRef.current >= MAX_CACHED_RECONNECT_CLOSES
          ) {
            handleFatalLobbyReset('未能恢复座位，请回到牌桌侧栏后重新进入可加入的牌局。', current.tableCode);
            return;
          }
          dispatch({ type: 'set_connection_status', status: 'reconnecting' });
          setStatusMessage('连接已断开，正在尝试恢复座位。');
          return;
        }

        dispatch({ type: 'set_connection_status', status: 'closed' });
      };
    },
  );

  useEffect(() => {
    if (
      state.connectionStatus !== 'reconnecting' ||
      !state.reconnectToken ||
      !state.tableCode ||
      !state.wsBaseUrl ||
      socketRef.current
    ) {
      return;
    }

    const wsBaseUrl = state.wsBaseUrl;

    const timeoutId = window.setTimeout(() => {
      openRoomSocket({
        tableCode: state.tableCode,
        nickname: currentUser?.display_name ?? state.nickname,
        wsBaseUrl,
        reconnectToken: state.reconnectToken,
        reconnect: true,
      });
    }, 1000);

    return () => {
      window.clearTimeout(timeoutId);
    };
  }, [currentUser?.display_name, openRoomSocket, state.connectionStatus, state.nickname, state.reconnectToken, state.tableCode, state.wsBaseUrl]);

  useEffect(() => {
    return () => {
      closeSocket(socketRef, heartbeatTimerRef);
    };
  }, []);

  useEffect(() => {
    const claimSelectionSignature = getClaimSelectionSignature(state);
    const previousClaimSelectionSignature = previousClaimSelectionSignatureRef.current;

    if (claimSelectionSignature && claimSelectionSignature !== previousClaimSelectionSignature) {
      const defaultSelection = getDefaultClaimCandidateSelection(state);

      if (defaultSelection) {
        dispatch({
          type: 'set_selected_tiles',
          tileIds: defaultSelection.tileIds,
          mode: defaultSelection.actionId,
        });
      } else if (state.selectedTileIds.length > 0) {
        dispatch({ type: 'set_selected_tiles', tileIds: [], mode: null });
      }
    } else if (!claimSelectionSignature && previousClaimSelectionSignature && state.selectedTileIds.length > 0) {
      dispatch({ type: 'set_selected_tiles', tileIds: [], mode: null });
    }

    previousClaimSelectionSignatureRef.current = claimSelectionSignature;
  }, [state]);

  const localTurnKongPromptSignature = getLocalTurnKongPromptSignature(state);
  const localTurnKongCandidateGroups = getLocalTurnKongCandidateGroups(state);
  const hasLocalTurnKongPrompt =
    localTurnKongPromptSignature !== null && localTurnKongPromptSignature !== dismissedLocalTurnKongPromptSignature;
  const localSelfHuPromptSignature = getLocalSelfHuPromptSignature(state);
  const isLocalSelfHuPromptDismissed =
    localSelfHuPromptSignature !== null && localSelfHuPromptSignature === dismissedLocalSelfHuPromptSignature;
  const hasLocalSelfHuPassOption = localSelfHuPromptSignature !== null && !isLocalSelfHuPromptDismissed;
  const lobbyBusy =
    authStatus === 'loading' ||
    state.connectionStatus === 'connecting' ||
    state.connectionStatus === 'reconnecting';
  const isSpectator = state.clientMode === 'spectator';
  const spectatorFocusSeat = isSpectator ? resolveSpectatorFocusSeat(state) : null;
  const spectatorFocusName =
    isSpectator
      ? state.roomSnapshot?.payload.seats.find((seat) => seat.seat_index === spectatorFocusSeat)?.nickname ?? null
      : null;
  const localSeatIndex = state.roomSnapshot?.payload.local_seat;
  const localSeatState =
    typeof localSeatIndex === 'number'
      ? state.roomSnapshot?.payload.seats.find((seat) => seat.seat_index === localSeatIndex)
      : null;
  const viewModel = createMatchViewModel(state, {
    showLocalTurnKongPrompt: !isSpectator && hasLocalTurnKongPrompt,
    showLocalSelfHuPassOption: !isSpectator && hasLocalSelfHuPassOption,
    hideLocalSelfHuPrompt: !isSpectator && isLocalSelfHuPromptDismissed,
    isSpectator,
    perspectiveSeat: spectatorFocusSeat,
  });
  const isLocalBotTakeoverEnabled = !isSpectator && Boolean(localSeatState?.is_bot);
  const roomOwnerUserId = state.roomSnapshot?.payload.owner_user_id ?? currentTableOwnerUserId;
  const onlineUsersForSidebar = leaderboard.filter((user) => onlineUserIds.includes(user.user_id));
  const tableSidebarPlayers: TableSidebarPlayer[] = viewModel.players.map((player) => {
    const matchedUser =
      (currentUser && currentUser.display_name === player.name ? currentUser : null) ??
      leaderboard.find((user) => user.display_name === player.name) ??
      null;

    return {
      key: `${player.seat}:${player.absoluteSeat ?? -1}`,
      seatLabel: getSeatLabel(player.absoluteSeat),
      displayLabel: matchedUser?.display_label ?? player.name,
      score: player.score,
      liveDelta: player.liveDelta,
      points: matchedUser?.points ?? null,
      connected: player.connected,
      isBotSeat: player.seatType === 'bot',
      isBotControlled: player.isBotControlled,
      profileUser: matchedUser,
    };
  });
  const snapshotSpectators = state.roomSnapshot?.payload.spectators ?? [];
  const tableSidebarSpectators: TableSidebarSpectator[] =
    snapshotSpectators.length > 0
      ? snapshotSpectators.map((spectator) => {
          const matchedUser =
            (currentUser && currentUser.user_id === spectator.user_id ? currentUser : null) ??
            leaderboard.find((user) => user.user_id === spectator.user_id) ??
            null;

          return {
            key: `spectator-${spectator.user_id}`,
            label: matchedUser?.display_label ?? spectator.display_name,
            subtitle: isSpectator && currentUser?.user_id === spectator.user_id ? '你正在观战该牌桌' : null,
          };
        })
      : isSpectator && currentUser
        ? [
            {
              key: `spectator-${currentUser.user_id}`,
              label: currentUser.display_label,
              subtitle: '你正在观战该牌桌',
            },
          ]
        : [];
  const isSidebarOwner = currentUser?.user_id !== undefined && roomOwnerUserId === currentUser.user_id;
  useSequentialBackgroundMusic(isBgmEnabled && state.roomSnapshot !== null);

  useEffect(() => {
    const previousLocalTurnKongPromptSignature = previousLocalTurnKongPromptSignatureRef.current;

    if (localTurnKongPromptSignature !== dismissedLocalTurnKongPromptSignature) {
      setDismissedLocalTurnKongPromptSignature((current) =>
        current === null || current === localTurnKongPromptSignature ? current : null,
      );
    }

    if (
      hasLocalTurnKongPrompt &&
      localTurnKongPromptSignature &&
      localTurnKongPromptSignature !== previousLocalTurnKongPromptSignature
    ) {
      const defaultGroup = localTurnKongCandidateGroups[0];

      if (defaultGroup) {
        dispatch({
          type: 'set_selected_tiles',
          tileIds: defaultGroup,
          mode: 'kong',
        });
      }
    }

    previousLocalTurnKongPromptSignatureRef.current = hasLocalTurnKongPrompt ? localTurnKongPromptSignature : null;
  }, [
    dismissedLocalTurnKongPromptSignature,
    hasLocalTurnKongPrompt,
    localTurnKongCandidateGroups,
    localTurnKongPromptSignature,
  ]);

  useEffect(() => {
    if (localSelfHuPromptSignature !== dismissedLocalSelfHuPromptSignature) {
      setDismissedLocalSelfHuPromptSignature((current) =>
        current === null || current === localSelfHuPromptSignature ? current : null,
      );
    }
  }, [dismissedLocalSelfHuPromptSignature, localSelfHuPromptSignature]);

  useEffect(() => {
    if (!isSpectator || !state.roomSnapshot) {
      return;
    }

    const nextSeat = resolveSpectatorFocusSeat(state);
    if (state.spectatorFocusSeat !== nextSeat) {
      dispatch({ type: 'set_spectator_focus_seat', seatIndex: nextSeat });
    }
  }, [isSpectator, state.roomSnapshot, state.spectatorFocusSeat]);

  function sendMessage(message: string) {
    if (!socketRef.current || socketRef.current.readyState !== WebSocket.OPEN) {
      setStatusMessage('当前尚未建立连接。');
      return false;
    }

    socketRef.current.send(message);
    return true;
  }

  async function handleLogin(value: { identifier: string; password: string }) {
    try {
      setAuthStatus('loading');
      setStatusMessage('正在登录...');
      const response = await loginWithPassword(defaults.apiBaseUrl, value);
      const nextSession = {
        sessionToken: response.session_token,
        user: response.user,
      };
      saveStoredAuthSession(nextSession);
      setAuthSession(nextSession);
      setCurrentUser(response.user);
      setSelectedProfileUser(response.user);
      setSelectedProfileFallbackName(response.user.display_name);
      dispatch({ type: 'set_credentials', nickname: response.user.display_name });
      setStatusMessage(null);
    } catch (error) {
      clearStoredAuthSession();
      setAuthSession(null);
      setAuthStatus('anonymous');
      setStatusMessage(error instanceof Error ? getSocialStatusCopy(error.message) : '登录失败。');
    }
  }

  async function handleRegister(value: { inviteCode: string; displayName: string; password: string }) {
    try {
      setAuthStatus('loading');
      setStatusMessage('正在注册...');
      const response = await registerWithInvite(defaults.apiBaseUrl, value);
      const nextSession = {
        sessionToken: response.session_token,
        user: response.user,
      };
      saveStoredAuthSession(nextSession);
      setAuthSession(nextSession);
      setCurrentUser(response.user);
      setSelectedProfileUser(response.user);
      setSelectedProfileFallbackName(response.user.display_name);
      dispatch({ type: 'set_credentials', nickname: response.user.display_name });
      setStatusMessage(null);
    } catch (error) {
      clearStoredAuthSession();
      setAuthSession(null);
      setAuthStatus('anonymous');
      setStatusMessage(error instanceof Error ? getSocialStatusCopy(error.message) : '注册失败。');
    }
  }

  async function handleCreateLobbyTable() {
    if (!authSession?.sessionToken || !currentUser) {
      setStatusMessage('请先登录。');
      return;
    }

    try {
      setStatusMessage('正在创建牌局...');
      dispatch({ type: 'set_config', apiBaseUrl: defaults.apiBaseUrl, wsBaseUrl: defaults.wsBaseUrl });
      const table = await createSocialTable(defaults.apiBaseUrl, authSession.sessionToken);
      setActiveLobbyTableCode(table.table_code);
      setCurrentTableOwnerUserId(table.owner_user_id ?? currentUser.user_id);
      setStatusMessage(`已创建牌局 ${table.table_code}，正在进入牌桌...`);
      openRoomSocket({
        tableCode: table.table_code,
        nickname: currentUser.display_name,
        wsBaseUrl: defaults.wsBaseUrl,
        sessionToken: authSession.sessionToken,
      });
    } catch (error) {
      setStatusMessage(error instanceof Error ? getSocialStatusCopy(error.message) : '创建牌局失败。');
      dispatch({ type: 'set_connection_status', status: 'error' });
    }
  }

  function handleEnterCreatedTable() {
    if (!authSession?.sessionToken || !currentUser || !activeLobbyTableCode) {
      return;
    }

    setStatusMessage('正在进入牌桌...');
    dispatch({ type: 'set_config', apiBaseUrl: defaults.apiBaseUrl, wsBaseUrl: defaults.wsBaseUrl });
    openRoomSocket({
      tableCode: activeLobbyTableCode,
      nickname: currentUser.display_name,
      wsBaseUrl: defaults.wsBaseUrl,
      sessionToken: authSession.sessionToken,
    });
  }

  async function handleInvitePlayer(userId: number) {
    if (!authSession?.sessionToken || !activeLobbyTableCode) {
      setStatusMessage('请先创建牌局。');
      return;
    }

    try {
      await createTableInvite(defaults.apiBaseUrl, authSession.sessionToken, activeLobbyTableCode, userId);
      setStatusMessage(`已向玩家 ${userId} 发出邀请。`);
    } catch (error) {
      setStatusMessage(error instanceof Error ? getSocialStatusCopy(error.message) : '邀请失败。');
    }
  }

  async function handleAcceptInvite(invite: TableInvite) {
    if (!authSession?.sessionToken || !currentUser) {
      setStatusMessage('请先登录。');
      return;
    }

    try {
      setStatusMessage(`正在进入牌桌 ${invite.table_code}...`);
      const accepted = await acceptTableInvite(defaults.apiBaseUrl, authSession.sessionToken, invite.id);
      setPendingInvites((current) => removeInviteById(current, invite.id));
      setInviteDialog((current) => (current?.id === invite.id ? null : current));
      setActiveLobbyTableCode(null);
      setCurrentTableOwnerUserId(invite.inviter_user_id);
      dispatch({ type: 'set_config', apiBaseUrl: defaults.apiBaseUrl, wsBaseUrl: defaults.wsBaseUrl });
      openRoomSocket({
        tableCode: accepted.table_code,
        nickname: currentUser.display_name,
        wsBaseUrl: defaults.wsBaseUrl,
        sessionToken: authSession.sessionToken,
      });
    } catch (error) {
      if (isStaleTableInviteError(error)) {
        setPendingInvites((current) => removeInviteById(current, invite.id));
        setInviteDialog((current) => (current?.id === invite.id ? null : current));
      }
      setStatusMessage(error instanceof Error ? getSocialStatusCopy(error.message) : '接受邀请失败。');
    }
  }

  async function handleLogout() {
    try {
      if (authSession?.sessionToken) {
        await logoutSession(defaults.apiBaseUrl, authSession.sessionToken);
      }
    } catch {
      // Ignore logout transport failures and clear local state regardless.
    }

    clearStoredAuthSession();
    clearStoredSession();
    setAuthSession(null);
    setCurrentUser(null);
    setLeaderboard([]);
    setOnlineUserIds([]);
    setPendingInvites([]);
    setPendingSpectatorRequests([]);
    setInviteDialog(null);
    setActiveLobbyTableCode(null);
    setCurrentTableOwnerUserId(null);
    setSelectedProfileUser(null);
    setSelectedProfileFallbackName(null);
    setProfileFanStats([]);
    setProfileRecentGames([]);
    setProfileMessage(null);
    setAuthStatus('anonymous');
    setStatusMessage(null);
    dispatch({ type: 'return_to_lobby', keepNickname: false, tableCode: '' });
    closeSocket(socketRef, heartbeatTimerRef);
    if (meSocketRef.current) {
      meSocketRef.current.onclose = null;
      meSocketRef.current.close();
      meSocketRef.current = null;
    }
  }

  function handleSelectSidebarUser(user: PublicUser) {
    setSelectedProfileUser(user);
    setSelectedProfileFallbackName(user.display_name);
  }

  async function handleApproveSpectatorRequest(requestId: number) {
    if (!authSession?.sessionToken) {
      setStatusMessage('请先登录。');
      return;
    }

    try {
      await approveSpectatorRequest(defaults.apiBaseUrl, authSession.sessionToken, requestId);
      setPendingSpectatorRequests((current) => removeSpectatorRequestById(current, requestId));
      setStatusMessage('已同意观战申请。');
    } catch (error) {
      setStatusMessage(error instanceof Error ? getSocialStatusCopy(error.message) : '处理观战申请失败。');
    }
  }

  async function handleRejectSpectatorRequest(requestId: number) {
    if (!authSession?.sessionToken) {
      setStatusMessage('请先登录。');
      return;
    }

    try {
      await rejectSpectatorRequest(defaults.apiBaseUrl, authSession.sessionToken, requestId);
      setPendingSpectatorRequests((current) => removeSpectatorRequestById(current, requestId));
      setStatusMessage('已拒绝观战申请。');
    } catch (error) {
      setStatusMessage(error instanceof Error ? getSocialStatusCopy(error.message) : '处理观战申请失败。');
    }
  }

  function handleSwitchSpectatorPerspective() {
    const seats = getOccupiedSpectatorSeats(state.roomSnapshot);
    if (seats.length === 0) {
      return;
    }

    const current = resolveSpectatorFocusSeat(state);
    const currentIndex = seats.indexOf(current);
    const nextSeat = seats[(currentIndex + 1) % seats.length] ?? seats[0];
    dispatch({ type: 'set_spectator_focus_seat', seatIndex: nextSeat });
  }

  function handleTileSelect(tileId: string) {
    if (isSpectator || isLocalBotTakeoverEnabled) {
      return;
    }

    if (state.optimisticDiscard) {
      return;
    }

    if (
      canUseClaimMultiSelect(state) ||
      hasLocalTurnKongPrompt ||
      state.selectionMode === 'kong' ||
      state.selectionMode === 'chow' ||
      state.selectionMode === 'pung'
    ) {
      const nextTileIds = state.selectedTileIds.includes(tileId)
        ? state.selectedTileIds.filter((selectedTileId) => selectedTileId !== tileId)
        : [...state.selectedTileIds, tileId];

      dispatch({
        type: 'set_selected_tiles',
        tileIds: nextTileIds,
        mode: hasLocalTurnKongPrompt ? 'kong' : state.selectionMode,
      });
      return;
    }

    const isAlreadySingleSelected = state.selectedTileIds.length === 1 && state.selectedTileIds[0] === tileId;
    dispatch({
      type: 'set_selected_tiles',
      tileIds: isAlreadySingleSelected ? [] : [tileId],
      mode: 'single',
    });
  }

  function handleAction(actionId: BattleActionId) {
    if (isSpectator || (isLocalBotTakeoverEnabled && !BOT_TAKEOVER_ROOM_ACTION_IDS.has(actionId))) {
      return;
    }

    if (state.optimisticDiscard && isActionBlockedByOptimisticDiscard(actionId)) {
      return;
    }

    if (actionId === 'ready') {
      const localSeat = state.roomSnapshot?.payload.local_seat;
      const localSeatState =
        typeof localSeat === 'number'
          ? state.roomSnapshot?.payload.seats.find((seat) => seat.seat_index === localSeat)
          : null;
      sendMessage(serializeClientMessage(createReadyMessage(!localSeatState?.ready)));
      return;
    }

    if (actionId === 'start_match') {
      sendMessage(serializeClientMessage(createStartMatchMessage()));
      return;
    }

    if (actionId === 'start_next_round') {
      sendMessage(serializeClientMessage(createStartNextRoundMessage()));
      return;
    }

    if (actionId === 'restart_match') {
      sendMessage(serializeClientMessage(createRestartMatchMessage()));
      return;
    }

    if (actionId === 'discard') {
      if (state.selectedTileIds.length !== 1) {
        return;
      }

      const discardTileId = state.selectedTileIds[0];
      if (!sendMessage(serializeClientMessage(createActionRequestMessage(actionId, [discardTileId])))) {
        return;
      }

      dispatch({
        type: 'queue_optimistic_discard',
        tileId: discardTileId,
        actionType: 'discard',
      });
      dispatch({ type: 'set_selected_tiles', tileIds: [], mode: null });
      return;
    }

    if (actionId === 'ready_hand') {
      if (state.selectedTileIds.length !== 1) {
        return;
      }

      const discardTileId = state.selectedTileIds[0];
      if (!sendMessage(serializeClientMessage(createActionRequestMessage(actionId, [discardTileId])))) {
        return;
      }

      dispatch({
        type: 'queue_optimistic_discard',
        tileId: discardTileId,
        actionType: 'ready_hand',
      });
      dispatch({ type: 'set_selected_tiles', tileIds: [], mode: null });
      return;
    }

    if (actionId === 'flower') {
      const flowerTileIds = getFlowerCandidateTileIds(state);
      if (state.selectedTileIds.length === 1 && flowerTileIds.includes(state.selectedTileIds[0])) {
        if (!sendMessage(serializeClientMessage(createActionRequestMessage(actionId, state.selectedTileIds)))) {
          return;
        }

        dispatch({ type: 'queue_optimistic_flower', tileId: state.selectedTileIds[0] });
        dispatch({ type: 'set_selected_tiles', tileIds: [], mode: null });
        return;
      }

      if (flowerTileIds.length === 1) {
        if (!sendMessage(serializeClientMessage(createActionRequestMessage(actionId, flowerTileIds)))) {
          return;
        }

        dispatch({ type: 'queue_optimistic_flower', tileId: flowerTileIds[0] });
        return;
      }

      if (flowerTileIds.length > 1) {
        dispatch({
          type: 'set_selected_tiles',
          tileIds: [flowerTileIds[0]],
          mode: 'single',
        });
      }
      return;
    }

    if (actionId === 'kong' || actionId === 'chow' || actionId === 'pung') {
      const candidateGroups =
        actionId === 'kong' && hasLocalTurnKongPrompt
          ? localTurnKongCandidateGroups
          : getActionCandidateGroups(state, actionId);
      const matchingGroup = getMatchingActionGroup(state.selectedTileIds, candidateGroups);

      if (matchingGroup) {
        sendMessage(serializeClientMessage(createActionRequestMessage(actionId, matchingGroup)));
        dispatch({ type: 'set_selected_tiles', tileIds: [], mode: null });
        return;
      }

      return;
    }

    if (actionId === 'pass') {
      if (hasLocalSelfHuPassOption && localSelfHuPromptSignature) {
        if (!sendMessage(serializeClientMessage(createActionRequestMessage(actionId)))) {
          return;
        }
        setDismissedLocalSelfHuPromptSignature(localSelfHuPromptSignature);
        if (hasLocalTurnKongPrompt && localTurnKongPromptSignature) {
          setDismissedLocalTurnKongPromptSignature(localTurnKongPromptSignature);
        }
        dispatch({ type: 'set_selected_tiles', tileIds: [], mode: null });
        return;
      }

      let handledLocalPass = false;

      if (hasLocalTurnKongPrompt && localTurnKongPromptSignature) {
        setDismissedLocalTurnKongPromptSignature(localTurnKongPromptSignature);
        handledLocalPass = true;
      }

      if (handledLocalPass) {
        dispatch({ type: 'set_selected_tiles', tileIds: [], mode: null });
        return;
      }

      sendMessage(serializeClientMessage(createActionRequestMessage(actionId)));
      dispatch({ type: 'set_selected_tiles', tileIds: [], mode: null });
      return;
    }

    sendMessage(serializeClientMessage(createActionRequestMessage(actionId as BackendActionType)));
  }

  function handleClaimCandidateSelect(actionId: ClaimActionId, tileIds: string[]) {
    if (isSpectator || isLocalBotTakeoverEnabled) {
      return;
    }

    dispatch({
      type: 'set_selected_tiles',
      tileIds,
      mode: actionId,
    });
  }

  function handleClaimCandidateActivate(actionId: ClaimActionId, tileIds: string[]) {
    if (isSpectator || isLocalBotTakeoverEnabled) {
      return;
    }

    if (state.optimisticDiscard) {
      return;
    }

    if (!sendMessage(serializeClientMessage(createActionRequestMessage(actionId, tileIds)))) {
      return;
    }

    dispatch({ type: 'set_selected_tiles', tileIds: [], mode: null });
  }

  function handleQuickChat(targetSeat: number, emoji: QuickChatEmoji) {
    if (isSpectator) {
      return;
    }

    sendMessage(serializeClientMessage(createQuickChatMessage(targetSeat, emoji)));
  }

  function handleAdjustBots(delta: 1 | -1) {
    if (isSpectator) {
      return;
    }

    sendMessage(serializeClientMessage(createAdjustBotsMessage(delta)));
  }

  function handleSetBotTakeover(enabled: boolean) {
    if (isSpectator) {
      return;
    }

    sendMessage(serializeClientMessage(createSetBotTakeoverMessage(enabled)));
  }

  function handleTileDoubleClick(tileId: string) {
    if (isSpectator || isLocalBotTakeoverEnabled) {
      return;
    }

    if (!canQuickDiscard(state, hasLocalTurnKongPrompt)) {
      return;
    }

    if (!sendMessage(serializeClientMessage(createActionRequestMessage('discard', [tileId])))) {
      return;
    }

    dispatch({
      type: 'queue_optimistic_discard',
      tileId,
      actionType: 'discard',
    });
    dispatch({ type: 'set_selected_tiles', tileIds: [], mode: null });
  }

  function handleCopyTableCode() {
    if (!state.tableCode && !activeLobbyTableCode) {
      return;
    }

    const tableCode = state.tableCode || activeLobbyTableCode || '';
    if (navigator.clipboard?.writeText) {
      navigator.clipboard.writeText(tableCode).catch(() => undefined);
    }
    setStatusMessage(`已复制牌桌编号 ${tableCode}。`);
  }

  function handleLeaveTable() {
    if (isSpectator) {
      handleLeaveToLobby(state.roomSnapshot?.payload.table_code ?? state.tableCode);
      return;
    }

    const shouldConfirmLeave = state.roomSnapshot?.payload.phase !== 'waiting';
    if (shouldConfirmLeave && !window.confirm(LEAVE_TABLE_CONFIRM_MESSAGE)) {
      return;
    }

    if (!socketRef.current || socketRef.current.readyState !== WebSocket.OPEN) {
      handleLeaveToLobby(
        state.roomSnapshot?.payload.table_code ?? state.tableCode,
        '当前连接已断开，已回到牌桌界面。若仍需回到牌局，请等待房主重新邀请。',
      );
      return;
    }

    leavingTableRef.current = true;
    socketRef.current.send(serializeClientMessage(createLeaveTableMessage()));
  }

  const sidebarRoomPanel = currentUser ? (
    <SocialSidebarPanel
      currentUser={currentUser}
      leaderboard={leaderboard}
      onlineUserIds={onlineUserIds}
      pendingInvites={pendingInvites}
      activeTableCode={activeLobbyTableCode}
      inviteDialog={inviteDialog}
      busy={lobbyBusy}
      message={statusMessage}
      onCreateTable={handleCreateLobbyTable}
      onEnterTable={handleEnterCreatedTable}
      onInvite={handleInvitePlayer}
      onAcceptInvite={handleAcceptInvite}
      onDismissInviteDialog={() => setInviteDialog(null)}
      onLogout={handleLogout}
    />
  ) : null;

  function renderBattleScreen(options: { defaultSidebarOpen?: boolean; initialSidebarTab?: 'room' | 'players' } = {}) {
    return (
      <BattleScreen
        isBgmEnabled={isBgmEnabled}
        onToggleBgm={() =>
          setIsBgmEnabled((current) => {
            const next = !current;
            saveStoredBgmEnabled(next);
            return next;
          })
        }
        isVoiceEnabled={isVoiceEnabled}
        onToggleVoice={() => setIsVoiceEnabled((current) => !current)}
        isBotTakeoverEnabled={isLocalBotTakeoverEnabled}
        onToggleBotTakeover={handleSetBotTakeover}
        sidebarRoomPanel={sidebarRoomPanel}
        sidebarDefaultOpen={options.defaultSidebarOpen}
        sidebarInitialTab={options.initialSidebarTab}
        sidebarPlayers={tableSidebarPlayers}
        sidebarOnlineUsers={onlineUsersForSidebar}
        sidebarSpectators={tableSidebarSpectators}
        sidebarProfileUser={selectedProfileUser}
        sidebarProfileFallbackName={selectedProfileFallbackName}
        sidebarProfileFanStats={profileFanStats}
        sidebarProfileRecentGames={profileRecentGames}
        sidebarProfileLoading={profileLoading}
        sidebarProfileMessage={profileMessage}
        sidebarSpectatorRequests={pendingSpectatorRequests}
        sidebarTabAlerts={{
          room: pendingInvites.length > 0,
          requests: isSidebarOwner && pendingSpectatorRequests.length > 0,
        }}
        isSidebarOwner={isSidebarOwner}
        onSidebarSelectUser={handleSelectSidebarUser}
        onApproveSpectatorRequest={handleApproveSpectatorRequest}
        onRejectSpectatorRequest={handleRejectSpectatorRequest}
        viewModel={viewModel}
        themeId={themeId}
        themeLabel={getThemeLabel(themeId)}
        onCycleTheme={() => setThemeId((currentThemeId) => getNextThemeId(currentThemeId))}
        onTileSelect={handleTileSelect}
        onTileDoubleClick={handleTileDoubleClick}
        onClaimCandidateSelect={handleClaimCandidateSelect}
        onClaimCandidateActivate={handleClaimCandidateActivate}
        onAction={handleAction}
        onCopyTableCode={handleCopyTableCode}
        onLeaveTable={handleLeaveTable}
        onAddBot={() => handleAdjustBots(1)}
        onRemoveBot={() => handleAdjustBots(-1)}
        onQuickChat={handleQuickChat}
        isSpectator={isSpectator}
        spectatorFocusName={spectatorFocusName}
        onSwitchSpectatorPerspective={isSpectator ? handleSwitchSpectatorPerspective : undefined}
      />
    );
  }

  if (!state.roomSnapshot) {
    if (authStatus !== 'ready' || !currentUser) {
      return (
        <AuthGate
          status={authStatus === 'loading' ? 'loading' : statusMessage ? 'error' : 'idle'}
          message={statusMessage}
          onLogin={handleLogin}
          onRegister={handleRegister}
        />
      );
    }

    return renderBattleScreen({ defaultSidebarOpen: true, initialSidebarTab: 'room' });
  }

  return renderBattleScreen();
}
