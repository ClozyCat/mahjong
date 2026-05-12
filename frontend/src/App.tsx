import { useEffect, useEffectEvent, useMemo, useReducer, useRef, useState, type MutableRefObject } from 'react';

import { AuthGate } from './components/auth/AuthGate';
import { BattleScreen } from './components/battle-screen/BattleScreen';
import type { TableSidebarPlayer } from './components/table-sidebar/TableSidebar';
import { SocialSidebarMessagesPanel, SocialSidebarPanel } from './components/lobby/SocialSidebarPanel';
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
  createRestartMatchMessage,
  createSetBotTakeoverMessage,
  createStartMatchMessage,
  createStartNextRoundMessage,
  parseServerMessage,
  serializeClientMessage,
} from './lib/socket';
import { buildMeSocketUrl, parseSocialServerMessage } from './lib/meSocket';
import {
  acceptTableInvite,
  createSocialTable,
  createTableInvite,
  getLeaderboard,
  getMyActiveTable,
  getMyInvites,
  rejectTableInvite,
} from './lib/socialApi';
import { createInitialSessionState, sessionReducer } from './lib/sessionReducer';
import { titleForPoints } from './lib/systemBroadcastCopy';
import {
  clearStoredSession,
  loadStoredThemeId,
  saveStoredThemeId,
  loadStoredBgmEnabled,
  saveStoredBgmEnabled,
  loadStoredVoiceEnabled,
  saveStoredVoiceEnabled,
} from './lib/storage';
import { DEFAULT_THEME_ID, getNextThemeId, getRandomThemeId, getThemeLabel, isThemeId } from './lib/themes';
import type {
  BackendActionType,
  BattleActionId,
  ClaimActionId,
  PublicUser,
  QuickChatEmoji,
  SessionState,
  TableInvite,
} from './types/match';

const HEARTBEAT_INTERVAL_MS = 20_000;
const SOCIAL_REFRESH_INTERVAL_MS = 15_000;
const SOCIAL_SOCKET_RECONNECT_MS = 1_000;
const TABLE_SEAT_CAPACITY = 4;
const ACTIVE_TABLE_LOOKUP_MESSAGE = '正在检查当前账号所在牌桌...';
const ACTIVE_TABLE_RETRY_MESSAGE = '牌桌连接已断开，正在重连你当前所在的牌桌。';
const CLAIM_ACTION_IDS = ['chow', 'pung', 'kong'] as const;
const CLAIM_RESPONSE_ACTION_IDS = ['chow', 'pung', 'kong', 'hu'] as const;
const BOT_TAKEOVER_ROOM_ACTION_IDS = new Set<BattleActionId>([
  'ready',
  'start_match',
  'start_next_round',
  'restart_match',
]);
type AuthStatus = 'loading' | 'anonymous' | 'ready';
type SentInviteStatus = 'pending' | 'rejected';
type RoomSocketOptions = {
  tableCode: string;
  nickname: string;
  wsBaseUrl: string;
  sessionToken?: string | null;
  reconnect?: boolean;
};

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
  const storedAuthSession = loadStoredAuthSession();

  return {
    defaults,
    storedAuthSession,
  };
}

function getRejectedMessage(reason: string) {
  const lookup: Record<string, string> = {
    table_not_found: '牌桌不存在或已关闭。',
    table_closed: '牌桌已关闭，请返回大厅重新进入。',
    table_full: '本牌局人数已满。',
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
    table_multiplier_locked: '牌局已开始，无法再修改牌局设置。',
    table_not_found: '牌桌不存在或已关闭。',
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
  return CLAIM_RESPONSE_ACTION_IDS.some((actionId) => options.includes(actionId));
}

function getClaimSelectionSignature(state: SessionState) {
  const pendingAction = state.roomSnapshot?.payload.private_state?.pending_action;

  if (pendingAction?.type === 'claim_window' && Array.isArray(pendingAction.options)) {
    const options = pendingAction.options
      .filter((option): option is BackendActionType => typeof option === 'string')
      .filter((option): option is (typeof CLAIM_RESPONSE_ACTION_IDS)[number] =>
        CLAIM_RESPONSE_ACTION_IDS.includes(option as (typeof CLAIM_RESPONSE_ACTION_IDS)[number]),
      );
    return options.length > 0 ? `claim:${pendingAction.deadline_at}:${options.slice().sort().join(',')}` : null;
  }

  const promptOptions = (state.latestActionPrompt?.payload.options ?? []).filter(
    (option): option is BackendActionType =>
      CLAIM_RESPONSE_ACTION_IDS.includes(option as (typeof CLAIM_RESPONSE_ACTION_IDS)[number]) || option === 'pass',
  );
  if (promptOptions.includes('pass') && hasClaimAction(promptOptions)) {
    const options = promptOptions.filter((option): option is (typeof CLAIM_RESPONSE_ACTION_IDS)[number] =>
      CLAIM_RESPONSE_ACTION_IDS.includes(option as (typeof CLAIM_RESPONSE_ACTION_IDS)[number]),
    );
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

function isWaitingInNonAllBotRoom(snapshot: SessionState['roomSnapshot']) {
  const payload = snapshot?.payload;
  if (!payload || payload.phase !== 'waiting' || payload.seats.length === 0) {
    return false;
  }

  return payload.seats.some((seat) => {
    if (seat.seat_type) {
      return seat.seat_type !== 'bot';
    }

    return !seat.is_bot;
  });
}

function isStandaloneBotSeat(seat: { seat_type?: string; is_bot?: boolean }) {
  if (seat.seat_type) {
    return seat.seat_type === 'bot';
  }

  return Boolean(seat.is_bot);
}

function hasInviteableTableSeat(snapshot: SessionState['roomSnapshot'], tableCode: string | null) {
  const payload = snapshot?.payload;
  if (!payload || !tableCode || payload.table_code !== tableCode) {
    return false;
  }

  if (payload.seats.some(isStandaloneBotSeat)) {
    return true;
  }

  if (payload.phase === 'waiting' && payload.seats.length < TABLE_SEAT_CAPACITY) {
    return true;
  }

  return false;
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

  return [
    nextInvite,
    ...current.filter(
      (invite) => invite.id !== nextInvite.id && invite.inviter_user_id !== nextInvite.inviter_user_id,
    ),
  ];
}

function removeInviteById(current: TableInvite[], inviteId: number) {
  return current.filter((invite) => invite.id !== inviteId);
}

function isPendingTableInvite(invite: TableInvite) {
  return invite.status === 'pending';
}

function getPendingTableInvites(invites: TableInvite[]) {
  return invites
    .filter(isPendingTableInvite)
    .reduceRight<TableInvite[]>((current, invite) => upsertInvite(current, invite), []);
}

function updateUserPoints(user: PublicUser | null, userId: number, points: number, title?: string) {
  if (!user || user.user_id !== userId) {
    return user;
  }

  const nextTitle = title ?? titleForPoints(points);
  return {
    ...user,
    points,
    title: nextTitle,
    display_label: `${user.display_name}（${nextTitle}）`,
  };
}

function updateLeaderboardUserPoints(leaderboard: PublicUser[], userId: number, points: number, title?: string) {
  return leaderboard.map((user) => updateUserPoints(user, userId, points, title) ?? user);
}

function updateUserActiveTableCode(
  user: PublicUser | null,
  userId: number,
  tableCode: string | null,
  tablePhase?: PublicUser['active_table_phase'],
) {
  if (!user || user.user_id !== userId) {
    return user;
  }

  return {
    ...user,
    active_table_code: tableCode,
    active_table_phase: tableCode ? tablePhase ?? user.active_table_phase ?? null : null,
  };
}

function updateLeaderboardUserActiveTableCode(
  leaderboard: PublicUser[],
  userId: number,
  tableCode: string | null,
  tablePhase?: PublicUser['active_table_phase'],
) {
  return leaderboard.map((user) =>
    user.user_id === userId
      ? {
          ...user,
          active_table_code: tableCode,
          active_table_phase: tableCode ? tablePhase ?? user.active_table_phase ?? null : null,
        }
      : user,
  );
}

function isStaleTableInviteError(error: unknown) {
  return error instanceof Error && error.message === 'table_not_found';
}

function getSeatLabel(seatIndex?: number | null) {
  const windLabels = ['东位', '南位', '西位', '北位'];
  if (typeof seatIndex !== 'number' || seatIndex < 0) {
    return '未知座位';
  }

  return windLabels[seatIndex] ?? `${seatIndex + 1}号位`;
}

function getUserDisplayName(users: PublicUser[], userId: number) {
  return users.find((user) => user.user_id === userId)?.display_name ?? `用户 #${userId}`;
}

export default function App() {
  const [isBgmEnabled, setIsBgmEnabled] = useState(() => loadStoredBgmEnabled());
  const [isVoiceEnabled, setIsVoiceEnabled] = useState(() => loadStoredVoiceEnabled());
  const { defaults, storedAuthSession } = useMemo(getDefaultConfig, []);
  const [themeId, setThemeId] = useState(() => {
    const storedThemeId = loadStoredThemeId();
    const nextThemeId = isThemeId(storedThemeId) ? getRandomThemeId(storedThemeId) : getRandomThemeId();

    return isThemeId(nextThemeId) ? nextThemeId : DEFAULT_THEME_ID;
  });
  const [statusMessage, setStatusMessage] = useState<string | null>(null);
  const [authStatus, setAuthStatus] = useState<AuthStatus>(storedAuthSession ? 'loading' : 'anonymous');
  const [authSession, setAuthSession] = useState(storedAuthSession);
  const [currentUser, setCurrentUser] = useState<PublicUser | null>(storedAuthSession?.user ?? null);
  const [leaderboard, setLeaderboard] = useState<PublicUser[]>([]);
  const [onlineUserIds, setOnlineUserIds] = useState<number[]>([]);
  const [pendingInvites, setPendingInvites] = useState<TableInvite[]>([]);
  const [sentInviteStatusesByUserId, setSentInviteStatusesByUserId] = useState<Record<number, SentInviteStatus>>({});
  const [activeLobbyTableCode, setActiveLobbyTableCode] = useState<string | null>(null);
  const [isActiveTableLookupPending, setIsActiveTableLookupPending] = useState(false);
  const [state, dispatch] = useReducer(
    sessionReducer,
    undefined,
    (): SessionState => ({
      ...createInitialSessionState(),
      apiBaseUrl: defaults.apiBaseUrl,
      wsBaseUrl: defaults.wsBaseUrl,
      tableCode: '',
      nickname: storedAuthSession?.user.display_name ?? '',
      connectionStatus: 'idle',
    }),
  );
  const socketRef = useRef<WebSocket | null>(null);
  const meSocketRef = useRef<WebSocket | null>(null);
  const heartbeatTimerRef = useRef<number | null>(null);
  const sessionRef = useRef(state);
  const leavingTableRef = useRef(false);
  const reconnectCloseCountRef = useRef(0);
  const activeTableRestoreRef = useRef<{ tableCode: string; sessionToken: string; nickname: string } | null>(null);
  const skipActiveTableLookupTokenRef = useRef<string | null>(null);
  const previousClaimSelectionSignatureRef = useRef<string | null>(null);
  const previousLocalTurnKongPromptSignatureRef = useRef<string | null>(null);
  const previousHadRoomSnapshotRef = useRef(false);
  const [dismissedLocalTurnKongPromptSignature, setDismissedLocalTurnKongPromptSignature] = useState<string | null>(null);
  const [dismissedLocalSelfHuPromptSignature, setDismissedLocalSelfHuPromptSignature] = useState<string | null>(null);
  const [dismissedClaimPromptSignature, setDismissedClaimPromptSignature] = useState<string | null>(null);
  const inviteCreatorLabelsByUserId = useMemo(() => {
    const labelsByUserId: Record<number, string> = {};
    for (const user of leaderboard) {
      labelsByUserId[user.user_id] = user.display_label;
    }
    if (currentUser) {
      labelsByUserId[currentUser.user_id] = currentUser.display_label;
    }
    return labelsByUserId;
  }, [currentUser, leaderboard]);

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
        setSentInviteStatusesByUserId({});
        setIsActiveTableLookupPending(false);
        activeTableRestoreRef.current = null;
        skipActiveTableLookupTokenRef.current = null;
        if (meSocketRef.current) {
          meSocketRef.current.onclose = null;
          meSocketRef.current.close();
          meSocketRef.current = null;
        }
        return;
      }

      setAuthStatus(currentUser ? 'ready' : 'loading');

      try {
        const [me, nextInvites, nextLeaderboard] = await Promise.all([
          getMe(defaults.apiBaseUrl, authSession.sessionToken),
          getMyInvites(defaults.apiBaseUrl, authSession.sessionToken),
          getLeaderboard(defaults.apiBaseUrl),
        ]);

        if (cancelled) {
          return;
        }

        setCurrentUser(me);
        setPendingInvites(getPendingTableInvites(nextInvites));
        setSentInviteStatusesByUserId({});
        setLeaderboard(nextLeaderboard);
        setAuthStatus('ready');
        setStatusMessage((current) => {
          if (
            current === ACTIVE_TABLE_LOOKUP_MESSAGE ||
            current === ACTIVE_TABLE_RETRY_MESSAGE ||
            current?.includes('正在重连')
          ) {
            return current;
          }

          return null;
        });
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
        setIsActiveTableLookupPending(false);
        activeTableRestoreRef.current = null;
        skipActiveTableLookupTokenRef.current = null;
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
    if (authStatus !== 'ready' || !authSession?.sessionToken || !currentUser || !state.wsBaseUrl) {
      return;
    }

    const currentUserId = currentUser.user_id;
    const sessionToken = authSession.sessionToken;
    const wsBaseUrl = state.wsBaseUrl;
    let closed = false;
    let reconnectTimerId: number | null = null;
    let heartbeatTimerId: number | null = null;
    let refreshTimerId: number | null = null;

    function clearSocialTimers() {
      if (reconnectTimerId !== null) {
        window.clearTimeout(reconnectTimerId);
        reconnectTimerId = null;
      }
      if (heartbeatTimerId !== null) {
        window.clearInterval(heartbeatTimerId);
        heartbeatTimerId = null;
      }
      if (refreshTimerId !== null) {
        window.clearInterval(refreshTimerId);
        refreshTimerId = null;
      }
    }

    async function refreshSocialSidebarData(socket: WebSocket) {
      try {
        const [me, nextInvites, nextLeaderboard] = await Promise.all([
          getMe(defaults.apiBaseUrl, sessionToken),
          getMyInvites(defaults.apiBaseUrl, sessionToken),
          getLeaderboard(defaults.apiBaseUrl),
        ]);
        if (!closed && meSocketRef.current === socket) {
          const nextPendingInvites = getPendingTableInvites(nextInvites);
          setCurrentUser(me);
          setPendingInvites(nextPendingInvites);
          setLeaderboard(nextLeaderboard);
          saveStoredAuthSession({
            sessionToken,
            user: me,
          });
          setAuthSession((current) =>
            current && current.sessionToken === sessionToken
              ? {
                  sessionToken: current.sessionToken,
                  user: me,
                }
              : current,
          );
          dispatch({ type: 'set_credentials', nickname: me.display_name });
        }
      } catch {
        // Keep the last known sidebar state; the next websocket event or polling tick will retry.
      }
    }

    function handleSocialMessage(socket: WebSocket, raw: string) {
      const message = parseSocialServerMessage(raw);
      if (!message) {
        return;
      }

      if (message.type === 'user_presence_updated') {
        setOnlineUserIds(message.payload.online_user_ids);
        void refreshSocialSidebarData(socket);
        return;
      }

      if (message.type === 'user_points_updated') {
        const { user_id: userId, points, title } = message.payload;
        dispatch({ type: 'user_points_updated', message });
        setCurrentUser((current) => updateUserPoints(current, userId, points, title));
        setLeaderboard((current) => updateLeaderboardUserPoints(current, userId, points, title));
        setAuthSession((current) =>
          current && current.user.user_id === userId
            ? {
                ...current,
                user: updateUserPoints(current.user, userId, points, title) ?? current.user,
              }
            : current,
        );
        return;
      }

      if (message.type === 'user_active_table_updated') {
        const {
          user_id: userId,
          active_table_code: tableCode,
          active_table_phase: tablePhase = null,
        } = message.payload;
        setCurrentUser((current) => updateUserActiveTableCode(current, userId, tableCode, tablePhase));
        setLeaderboard((current) => updateLeaderboardUserActiveTableCode(current, userId, tableCode, tablePhase));
        setAuthSession((current) =>
          current && current.user.user_id === userId
            ? {
                ...current,
                user: {
                  ...current.user,
                  active_table_code: tableCode,
                  active_table_phase: tableCode ? tablePhase : null,
                },
              }
            : current,
        );
        return;
      }

      if (message.type === 'table_invite_created') {
        setPendingInvites((current) => upsertInvite(current, message.payload));
        return;
      }

      if (message.type === 'table_invite_decided') {
        setPendingInvites((current) => upsertInvite(current, message.payload));
        if (currentUserId === message.payload.inviter_user_id) {
          setSentInviteStatusesByUserId((current) => {
            if (message.payload.status === 'rejected') {
              return {
                ...current,
                [message.payload.invitee_user_id]: 'rejected',
              };
            }

            const next = { ...current };
            delete next[message.payload.invitee_user_id];
            return next;
          });
        }
        return;
      }

    }

    function openSocialSocket() {
      clearSocialTimers();
      const socket = new WebSocket(buildMeSocketUrl(wsBaseUrl, sessionToken));
      meSocketRef.current = socket;

      socket.onopen = () => {
        void refreshSocialSidebarData(socket);
        heartbeatTimerId = window.setInterval(() => {
          if (socket.readyState === WebSocket.OPEN) {
            socket.send(serializeClientMessage(createHeartbeatMessage(new Date().toISOString())));
          }
        }, HEARTBEAT_INTERVAL_MS);
        refreshTimerId = window.setInterval(() => {
          void refreshSocialSidebarData(socket);
        }, SOCIAL_REFRESH_INTERVAL_MS);
      };

      socket.onmessage = (event) => {
        handleSocialMessage(socket, String(event.data));
      };

      socket.onclose = () => {
        if (meSocketRef.current === socket) {
          meSocketRef.current = null;
        }
        clearSocialTimers();
        if (!closed) {
          reconnectTimerId = window.setTimeout(openSocialSocket, SOCIAL_SOCKET_RECONNECT_MS);
        }
      };
    }

    function refreshWhenVisible() {
      const socket = meSocketRef.current;
      if (!socket || document.visibilityState === 'hidden') {
        return;
      }
      void refreshSocialSidebarData(socket);
    }

    openSocialSocket();
    window.addEventListener('focus', refreshWhenVisible);
    document.addEventListener('visibilitychange', refreshWhenVisible);

    return () => {
      closed = true;
      clearSocialTimers();
      window.removeEventListener('focus', refreshWhenVisible);
      document.removeEventListener('visibilitychange', refreshWhenVisible);
      if (meSocketRef.current) {
        meSocketRef.current.onclose = null;
        meSocketRef.current.close();
        meSocketRef.current = null;
      }
    };
  }, [
    authSession?.sessionToken,
    authStatus,
    currentUser?.display_name,
    currentUser?.user_id,
    defaults.apiBaseUrl,
    defaults.wsBaseUrl,
    state.wsBaseUrl,
  ]);

  const handleLeaveToLobby = useEffectEvent((tableCode?: string, nextStatusMessage: string | null = null) => {
    leavingTableRef.current = false;
    reconnectCloseCountRef.current = 0;
    activeTableRestoreRef.current = null;
    setActiveLobbyTableCode(null);
    if (currentUser) {
      setCurrentUser((user) => updateUserActiveTableCode(user, currentUser.user_id, null));
      setLeaderboard((users) => updateLeaderboardUserActiveTableCode(users, currentUser.user_id, null));
    }
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
    activeTableRestoreRef.current = null;
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
      const isFatalTableClosed = message.payload.reason === 'table_closed';
      const isFatalJoinFailure = !current.roomSnapshot && message.payload.reason === 'table_full';

      if (leavingTableRef.current) {
        leavingTableRef.current = false;
      }

      if (isFatalTableMissing || isFatalTableClosed || isFatalJoinFailure) {
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
    (options: RoomSocketOptions) => {
      closeSocket(socketRef, heartbeatTimerRef);

      const {
        tableCode,
        nickname,
        wsBaseUrl,
        sessionToken,
        reconnect,
      } = options;
      if (!reconnect) {
        reconnectCloseCountRef.current = 0;
      }
      if (sessionToken) {
        activeTableRestoreRef.current = { tableCode, sessionToken, nickname };
      }
      dispatch({ type: 'set_connection_status', status: reconnect ? 'reconnecting' : 'connecting' });
      dispatch({ type: 'set_credentials', tableCode, nickname });
      dispatch({ type: 'set_config', wsBaseUrl });

      const socket = new WebSocket(buildWebSocketUrl(wsBaseUrl, tableCode));
      socketRef.current = socket;

      socket.onopen = () => {
        void (async () => {
          const message = createJoinTableMessage(sessionToken ?? '');
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
        const activeRestore = activeTableRestoreRef.current;
        if (activeRestore && activeRestore.tableCode === current.tableCode) {
          dispatch({ type: 'set_connection_status', status: 'reconnecting' });
          setStatusMessage(ACTIVE_TABLE_RETRY_MESSAGE);
          return;
        }

        dispatch({ type: 'set_connection_status', status: 'closed' });
      };
    },
  );
  const restoreActiveTable = useEffectEvent((tableCode: string, sessionToken: string, nickname: string) => {
    activeTableRestoreRef.current = { tableCode, sessionToken, nickname };
    reconnectCloseCountRef.current = 0;
    setActiveLobbyTableCode(tableCode);
    setStatusMessage(`检测到你正在牌桌 ${tableCode}，正在重连...`);
    dispatch({ type: 'set_config', apiBaseUrl: defaults.apiBaseUrl, wsBaseUrl: defaults.wsBaseUrl });
    dispatch({ type: 'set_credentials', tableCode, nickname });
    dispatch({ type: 'set_connection_status', status: 'reconnecting' });
    openRoomSocket({
      tableCode,
      nickname,
      wsBaseUrl: defaults.wsBaseUrl,
      sessionToken,
      reconnect: true,
    });
  });

  useEffect(() => {
    if (authStatus !== 'ready' || !authSession?.sessionToken || !currentUser) {
      return;
    }

    if (skipActiveTableLookupTokenRef.current === authSession.sessionToken) {
      return;
    }

    const sessionToken = authSession.sessionToken;
    const displayName = currentUser.display_name;
    let cancelled = false;
    setIsActiveTableLookupPending(true);
    setStatusMessage((current) => current ?? ACTIVE_TABLE_LOOKUP_MESSAGE);

    getMyActiveTable(defaults.apiBaseUrl, sessionToken)
      .then((activeTable) => {
        if (cancelled) {
          return;
        }
        if (activeTable) {
          restoreActiveTable(activeTable.table_code, sessionToken, displayName);
          return;
        }
        setStatusMessage((current) => (current === ACTIVE_TABLE_LOOKUP_MESSAGE ? null : current));
      })
      .catch((error) => {
        if (!cancelled) {
          setStatusMessage(error instanceof Error ? getSocialStatusCopy(error.message) : null);
        }
      })
      .finally(() => {
        if (!cancelled) {
          setIsActiveTableLookupPending(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [authSession?.sessionToken, authStatus, currentUser?.display_name, currentUser?.user_id, defaults.apiBaseUrl]);

  useEffect(() => {
    const activeRestore = activeTableRestoreRef.current;
    if (
      state.connectionStatus !== 'reconnecting' ||
      !activeRestore ||
      activeRestore.tableCode !== state.tableCode ||
      !state.wsBaseUrl ||
      socketRef.current
    ) {
      return;
    }

    const { tableCode, nickname, sessionToken } = activeRestore;
    const wsBaseUrl = state.wsBaseUrl;
    const timeoutId = window.setTimeout(() => {
      openRoomSocket({
        tableCode,
        nickname,
        wsBaseUrl,
        sessionToken,
        reconnect: true,
      });
    }, 1000);

    return () => {
      window.clearTimeout(timeoutId);
    };
  }, [openRoomSocket, state.connectionStatus, state.tableCode, state.wsBaseUrl]);

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
  const claimPromptSignature = getClaimSelectionSignature(state);
  const isClaimPromptDismissed = claimPromptSignature !== null && claimPromptSignature === dismissedClaimPromptSignature;
  const localTurnKongCandidateGroups = getLocalTurnKongCandidateGroups(state);
  const hasLocalTurnKongPrompt =
    localTurnKongPromptSignature !== null && localTurnKongPromptSignature !== dismissedLocalTurnKongPromptSignature;
  const localSelfHuPromptSignature = getLocalSelfHuPromptSignature(state);
  const isLocalSelfHuPromptDismissed =
    localSelfHuPromptSignature !== null && localSelfHuPromptSignature === dismissedLocalSelfHuPromptSignature;
  const hasLocalSelfHuPassOption = localSelfHuPromptSignature !== null && !isLocalSelfHuPromptDismissed;
  const lobbyBusy =
    authStatus === 'loading' ||
    isActiveTableLookupPending ||
    state.connectionStatus === 'connecting' ||
    state.connectionStatus === 'reconnecting';
  const isCreateTableBlockedByWaitingRoom = isWaitingInNonAllBotRoom(state.roomSnapshot);
  const isCreateTableDisabled = lobbyBusy || isCreateTableBlockedByWaitingRoom;
  const canInvitePlayers = hasInviteableTableSeat(state.roomSnapshot, activeLobbyTableCode);
  const creatingTableCodes =
    activeLobbyTableCode && state.roomSnapshot?.payload.table_code === activeLobbyTableCode && state.roomSnapshot.payload.phase === 'waiting'
      ? [activeLobbyTableCode]
      : [];
  const localSeatIndex = state.roomSnapshot?.payload.local_seat;
  const localSeatState =
    typeof localSeatIndex === 'number'
      ? state.roomSnapshot?.payload.seats.find((seat) => seat.seat_index === localSeatIndex)
      : null;
  const viewModel = createMatchViewModel(state, {
    showLocalTurnKongPrompt: hasLocalTurnKongPrompt,
    showLocalSelfHuPassOption: hasLocalSelfHuPassOption,
    hideLocalSelfHuPrompt: isLocalSelfHuPromptDismissed,
    hideLocalClaimPrompt: isClaimPromptDismissed,
  });
  const isLocalBotTakeoverEnabled = Boolean(localSeatState?.is_bot);
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
  useSequentialBackgroundMusic(isBgmEnabled && state.roomSnapshot !== null);

  useEffect(() => {
    if (claimPromptSignature !== dismissedClaimPromptSignature) {
      setDismissedClaimPromptSignature((current) =>
        current === null || current === claimPromptSignature ? current : null,
      );
    }
  }, [claimPromptSignature, dismissedClaimPromptSignature]);

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
      dispatch({ type: 'set_credentials', nickname: response.user.display_name });
      setAuthStatus('ready');
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
      skipActiveTableLookupTokenRef.current = nextSession.sessionToken;
      activeTableRestoreRef.current = null;
      clearStoredSession();
      dispatch({ type: 'return_to_lobby', keepNickname: false, tableCode: '' });
      closeSocket(socketRef, heartbeatTimerRef);
      saveStoredAuthSession(nextSession);
      setAuthSession(nextSession);
      setCurrentUser(response.user);
      dispatch({ type: 'set_credentials', nickname: response.user.display_name });
      setAuthStatus('ready');
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
    if (isCreateTableBlockedByWaitingRoom) {
      setStatusMessage('当前牌局正在等待开局，不能重复创建牌局。');
      return;
    }

    try {
      setStatusMessage('正在创建牌局...');
      dispatch({ type: 'set_config', apiBaseUrl: defaults.apiBaseUrl, wsBaseUrl: defaults.wsBaseUrl });
      const table = await createSocialTable(defaults.apiBaseUrl, authSession.sessionToken);
      setActiveLobbyTableCode(table.table_code);
      setSentInviteStatusesByUserId({});
      setCurrentUser((user) => updateUserActiveTableCode(user, currentUser.user_id, table.table_code, table.phase));
      setLeaderboard((users) =>
        updateLeaderboardUserActiveTableCode(users, currentUser.user_id, table.table_code, table.phase),
      );
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

  async function handleInvitePlayer(userId: number) {
    if (!authSession?.sessionToken || !activeLobbyTableCode) {
      setStatusMessage('请先创建牌局。');
      return;
    }

    if (!canInvitePlayers) {
      setStatusMessage('当前牌局没有可邀请的空座位或 BOT 座位。');
      return;
    }

    try {
      const invite = await createTableInvite(defaults.apiBaseUrl, authSession.sessionToken, activeLobbyTableCode, userId);
      setSentInviteStatusesByUserId((current) => ({
        ...current,
        [invite.invitee_user_id]: 'pending',
      }));
      setStatusMessage(`已向${getUserDisplayName(leaderboard, invite.invitee_user_id)}发出邀请。`);
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
      setActiveLobbyTableCode(null);
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
      }
      setStatusMessage(error instanceof Error ? getSocialStatusCopy(error.message) : '接受邀请失败。');
    }
  }

  async function handleRejectInvite(invite: TableInvite) {
    if (!authSession?.sessionToken) {
      setStatusMessage('请先登录。');
      return;
    }

    try {
      await rejectTableInvite(defaults.apiBaseUrl, authSession.sessionToken, invite.id);
      setPendingInvites((current) => removeInviteById(current, invite.id));
      setStatusMessage(`已拒绝牌桌 ${invite.table_code} 的邀请。`);
    } catch (error) {
      if (isStaleTableInviteError(error)) {
        setPendingInvites((current) => removeInviteById(current, invite.id));
      }
      setStatusMessage(error instanceof Error ? getSocialStatusCopy(error.message) : '拒绝邀请失败。');
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
    setSentInviteStatusesByUserId({});
    setActiveLobbyTableCode(null);
    setIsActiveTableLookupPending(false);
    activeTableRestoreRef.current = null;
    skipActiveTableLookupTokenRef.current = null;
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

  function handleTileSelect(tileId: string) {
    if (isLocalBotTakeoverEnabled) {
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
    if (isLocalBotTakeoverEnabled && !BOT_TAKEOVER_ROOM_ACTION_IDS.has(actionId)) {
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

      if (!sendMessage(serializeClientMessage(createActionRequestMessage(actionId)))) {
        return;
      }
      if (claimPromptSignature) {
        setDismissedClaimPromptSignature(claimPromptSignature);
      }
      dispatch({ type: 'set_selected_tiles', tileIds: [], mode: null });
      return;
    }

    sendMessage(serializeClientMessage(createActionRequestMessage(actionId as BackendActionType)));
  }

  function handleClaimCandidateSelect(actionId: ClaimActionId, tileIds: string[]) {
    if (isLocalBotTakeoverEnabled) {
      return;
    }

    dispatch({
      type: 'set_selected_tiles',
      tileIds,
      mode: actionId,
    });
  }

  function handleClaimCandidateActivate(actionId: ClaimActionId, tileIds: string[]) {
    if (isLocalBotTakeoverEnabled) {
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
    sendMessage(serializeClientMessage(createQuickChatMessage(targetSeat, emoji)));
  }

  function handleAdjustBots(delta: 1 | -1) {
    sendMessage(serializeClientMessage(createAdjustBotsMessage(delta)));
  }

  function handleSetBotTakeover(enabled: boolean) {
    sendMessage(serializeClientMessage(createSetBotTakeoverMessage(enabled)));
  }

  function handleTileDoubleClick(tileId: string) {
    if (isLocalBotTakeoverEnabled) {
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

  const hasMessageAlert = pendingInvites.length > 0;

  const sidebarRoomPanel = currentUser ? (
    <SocialSidebarPanel
      currentUser={currentUser}
      leaderboard={leaderboard}
      onlineUserIds={onlineUserIds}
      activeTableCode={activeLobbyTableCode}
      busy={lobbyBusy}
      isCreateTableDisabled={isCreateTableDisabled}
      canInvitePlayers={canInvitePlayers}
      inviteStatusesByUserId={sentInviteStatusesByUserId}
      message={statusMessage}
      onCreateTable={handleCreateLobbyTable}
      onInvite={handleInvitePlayer}
      onLogout={handleLogout}
    />
  ) : null;

  const sidebarMessagesPanel = currentUser ? (
    <SocialSidebarMessagesPanel
      pendingInvites={pendingInvites}
      inviteCreatorLabelsByUserId={inviteCreatorLabelsByUserId}
      message={statusMessage}
      onAcceptInvite={handleAcceptInvite}
      onRejectInvite={handleRejectInvite}
    />
  ) : null;

  function renderBattleScreen(options: { defaultSidebarOpen?: boolean; initialSidebarTab?: 'room' | 'online' } = {}) {
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
        onToggleVoice={() =>
          setIsVoiceEnabled((current) => {
            const next = !current;
            saveStoredVoiceEnabled(next);
            return next;
          })
        }
        isBotTakeoverEnabled={isLocalBotTakeoverEnabled}
        onToggleBotTakeover={handleSetBotTakeover}
        sidebarRoomPanel={sidebarRoomPanel}
        sidebarMessagesPanel={sidebarMessagesPanel}
        sidebarDefaultOpen={options.defaultSidebarOpen}
        sidebarInitialTab={options.initialSidebarTab}
        sidebarPlayers={tableSidebarPlayers}
        sidebarOnlineUsers={leaderboard}
        sidebarOnlineUserIds={onlineUserIds}
        sidebarCreatingTableCodes={creatingTableCodes}
        sidebarCurrentUserId={currentUser?.user_id ?? null}
        sidebarTabAlerts={{
          messages: hasMessageAlert,
        }}
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
