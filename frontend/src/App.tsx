import { lazy, Suspense, useEffect, useEffectEvent, useMemo, useReducer, useRef, useState } from 'react';

import {
  ACTIVE_TABLE_LOOKUP_MESSAGE,
  ACTIVE_TABLE_RETRY_MESSAGE,
  HEARTBEAT_INTERVAL_MS,
  SOCIAL_REFRESH_INTERVAL_MS,
  SOCIAL_SOCKET_RECONNECT_MS,
  getDefaultConfig,
  type AuthStatus,
  type RoomSocketOptions,
  type SentInviteStatus,
} from './app/config';
import { closeSocket } from './app/socketLifecycle';
import { getRejectedMessage, getSocialStatusCopy } from './app/statusCopy';
import {
  canQuickDiscard,
  canUseClaimMultiSelect,
  createInviteDialogUsers,
  createPendingWaitingRoomSnapshot,
  getClaimSelectionSignature,
  getDefaultClaimCandidateSelection,
  getInviteCreatorLabel,
  getPendingTableInvites,
  getUserDisplayName,
  hasInviteableTableSeat,
  isActionBlockedByOptimisticDiscard,
  isStaleTableInviteError,
  removeInviteById,
  updateLeaderboardUserActiveTableCode,
  updateLeaderboardUserPoints,
  updateUserActiveTableCode,
  updateUserPoints,
  upsertInvite,
} from './app/tableHelpers';
import { AuthGate } from './components/auth/AuthGate';
import {
  clearStoredAuthSession,
  getMe,
  loginWithPassword,
  logoutSession,
  registerWithInvite,
  saveStoredAuthSession,
} from './lib/authApi';
import { useSequentialBackgroundMusic } from './lib/backgroundMusic';
import {
  getActionCandidateGroups,
  getAutoPassKongCandidateTileKeys,
  getFlowerCandidateTileIds,
  getLocalTurnKongCandidateGroups,
  getLocalTurnKongPromptSignature,
  getMatchingActionGroup,
} from './lib/kongSelection';
import { createMatchViewModel, getLocalSelfHuPromptSignature } from './lib/matchViewModel';
import {
  buildWebSocketUrl,
  createAdjustBotsMessage,
  createActionRequestMessage,
  createHeartbeatMessage,
  createJoinTableMessage,
  createLeaveTableMessage,
  createQuickChatMessage,
  createPointGestureMessage,
  createSetMinimumHuFanMessage,
  createSetDealerDoubleMessage,
  createSetDealerRepeatMessage,
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
  createEvaluation,
  createTableInvite,
  getEvaluation,
  getEvaluationByTable,
  getLeaderboard,
  getMyActiveTable,
  getMyInvites,
  rejectTableInvite,
} from './lib/socialApi';
import { createInitialSessionState, sessionReducer } from './lib/sessionReducer';
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
  MinimumHuFan,
  PublicUser,
  QuickChatEmoji,
  SessionState,
  TableInvite,
  EvaluationSessionResponse,
} from './types/match';
const BOT_TAKEOVER_ROOM_ACTION_IDS = new Set<BattleActionId>([
  'start_match',
  'start_next_round',
]);
const EVALUATION_REFRESH_INTERVAL_MS = 1_500;
const BattleScreen = lazy(() =>
  import('./components/battle-screen/BattleScreen').then((module) => ({ default: module.BattleScreen })),
);

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
  const [evaluationSession, setEvaluationSession] = useState<EvaluationSessionResponse | null>(null);
  const [isEvaluationSubmitting, setIsEvaluationSubmitting] = useState(false);
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
  const previousClaimSelectionSignatureRef = useRef<string | null>(null);
  const previousLocalTurnKongPromptSignatureRef = useRef<string | null>(null);
  const previousHadRoomSnapshotRef = useRef(false);
  const [dismissedLocalTurnKongPromptSignature, setDismissedLocalTurnKongPromptSignature] = useState<string | null>(null);
  const [dismissedLocalSelfHuPromptSignature, setDismissedLocalSelfHuPromptSignature] = useState<string | null>(null);
  const [dismissedClaimPromptSignature, setDismissedClaimPromptSignature] = useState<string | null>(null);
  const [isAutoPassKongEnabled, setIsAutoPassKongEnabled] = useState(false);
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

    async function refreshSocialData(socket: WebSocket) {
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
        void refreshSocialData(socket);
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
        void refreshSocialData(socket);
        heartbeatTimerId = window.setInterval(() => {
          if (socket.readyState === WebSocket.OPEN) {
            socket.send(serializeClientMessage(createHeartbeatMessage(new Date().toISOString())));
          }
        }, HEARTBEAT_INTERVAL_MS);
        refreshTimerId = window.setInterval(() => {
          void refreshSocialData(socket);
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
      void refreshSocialData(socket);
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

  const refreshEvaluationSession = useEffectEvent(async () => {
    if (!authSession?.sessionToken || !evaluationSession) {
      return;
    }

    try {
      const session = await getEvaluation(defaults.apiBaseUrl, authSession.sessionToken, evaluationSession.evaluation_id);
      setEvaluationSession(session);
    } catch (error) {
      setStatusMessage(error instanceof Error ? getSocialStatusCopy(error.message) : '刷新评测失败。');
    }
  });

  useEffect(() => {
    if (!authSession?.sessionToken || !evaluationSession || evaluationSession.subjects.every((subject) => subject.completed)) {
      return;
    }

    const timerId = window.setInterval(() => {
      void refreshEvaluationSession();
    }, EVALUATION_REFRESH_INTERVAL_MS);

    return () => {
      window.clearInterval(timerId);
    };
  }, [authSession?.sessionToken, evaluationSession?.evaluation_id, evaluationSession?.subjects, refreshEvaluationSession]);

  const handleLeaveToLobby = useEffectEvent((tableCode?: string, nextStatusMessage: string | null = null) => {
    const leavingTableCode = tableCode ?? sessionRef.current.tableCode;
    leavingTableRef.current = false;
    reconnectCloseCountRef.current = 0;
    activeTableRestoreRef.current = null;
    if (evaluationSession?.subjects.some((subject) => subject.table_code === leavingTableCode)) {
      setEvaluationSession(null);
    }
    setActiveLobbyTableCode(null);
    if (currentUser) {
      setCurrentUser((user) => updateUserActiveTableCode(user, currentUser.user_id, null));
      setLeaderboard((users) => updateLeaderboardUserActiveTableCode(users, currentUser.user_id, null));
    }
    clearStoredSession();
    dispatch({
      type: 'return_to_lobby',
      tableCode: leavingTableCode,
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
      if (authSession?.sessionToken && currentUser) {
        void createAndEnterEmptyTable(authSession.sessionToken, currentUser.display_name);
      }
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
        onOpen,
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
          onOpen?.(socket);

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
          if (authSession?.sessionToken && currentUser) {
            void createAndEnterEmptyTable(authSession.sessionToken, currentUser.display_name);
          }
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

  const createAndEnterEmptyTable = useEffectEvent(async (
    sessionToken: string,
    nickname: string,
    isCancelled: () => boolean = () => false,
  ) => {
    if (!currentUser) {
      return;
    }

    try {
      setStatusMessage('正在为你准备空牌桌...');
      dispatch({ type: 'set_config', apiBaseUrl: defaults.apiBaseUrl, wsBaseUrl: defaults.wsBaseUrl });
      const table = await createSocialTable(defaults.apiBaseUrl, sessionToken);
      if (isCancelled()) {
        return;
      }
      dispatch({ type: 'set_room_snapshot', message: createPendingWaitingRoomSnapshot(table) });
      setActiveLobbyTableCode(table.table_code);
      setSentInviteStatusesByUserId({});
      setCurrentUser((user) => updateUserActiveTableCode(user, currentUser.user_id, table.table_code, table.phase));
      setLeaderboard((users) =>
        updateLeaderboardUserActiveTableCode(users, currentUser.user_id, table.table_code, table.phase),
      );
      openRoomSocket({
        tableCode: table.table_code,
        nickname,
        wsBaseUrl: defaults.wsBaseUrl,
        sessionToken,
      });
    } catch (error) {
      setStatusMessage(error instanceof Error ? getSocialStatusCopy(error.message) : '创建空牌桌失败。');
      dispatch({ type: 'set_connection_status', status: 'error' });
    }
  });

  useEffect(() => {
    if (authStatus !== 'ready' || !authSession?.sessionToken || !currentUser) {
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
        void createAndEnterEmptyTable(sessionToken, displayName, () => cancelled);
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
  const autoPassKongCandidateTileKeys = getAutoPassKongCandidateTileKeys(state);
  const canToggleAutoPassKong = autoPassKongCandidateTileKeys.length > 0;
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
  const canInvitePlayers = hasInviteableTableSeat(state.roomSnapshot);
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
  const inviteUsers = createInviteDialogUsers(leaderboard, onlineUserIds);
  const inviteHumanUsers = inviteUsers.filter(({ user }) => !user.is_special_bot);
  const inviteAiUsers = inviteUsers.filter(({ user }) => user.is_special_bot);
  useSequentialBackgroundMusic(isBgmEnabled && state.roomSnapshot !== null);

  useEffect(() => {
    if (canToggleAutoPassKong) {
      return;
    }

    setIsAutoPassKongEnabled(false);
  }, [canToggleAutoPassKong]);

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

  useEffect(() => {
    const promptOptions = new Set(viewModel.promptCue?.actionIds ?? []);
    const hasPureLocalKongPrompt =
      isAutoPassKongEnabled &&
      hasLocalTurnKongPrompt &&
      localTurnKongPromptSignature !== null &&
      viewModel.promptCue?.kind === 'turn_kong' &&
      promptOptions.has('kong') &&
      promptOptions.has('pass') &&
      !promptOptions.has('hu') &&
      !promptOptions.has('pung');

    if (!hasPureLocalKongPrompt) {
      return;
    }

    if (!sendMessage(serializeClientMessage(createActionRequestMessage('pass')))) {
      return;
    }

    setDismissedLocalTurnKongPromptSignature(localTurnKongPromptSignature);
    dispatch({ type: 'set_selected_tiles', tileIds: [], mode: null });
  }, [
    hasLocalTurnKongPrompt,
    isAutoPassKongEnabled,
    localTurnKongPromptSignature,
    viewModel.promptCue?.actionIds,
    viewModel.promptCue?.kind,
  ]);

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

  async function handleInvitePlayer(userId: number) {
    if (!authSession?.sessionToken || !activeLobbyTableCode) {
      setStatusMessage('正在准备你的空牌桌，请稍候。');
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

  async function handleCreateEvaluation(subjectUserIds: number[]) {
    if (!authSession?.sessionToken || !currentUser) {
      setStatusMessage('请先登录。');
      return;
    }

    try {
      setIsEvaluationSubmitting(true);
      const session = await createEvaluation(defaults.apiBaseUrl, authSession.sessionToken, subjectUserIds);
      setEvaluationSession(session);
      const selfSubject = session.subjects.find((subject) => subject.user_id === currentUser.user_id);
      if (selfSubject) {
        setActiveLobbyTableCode(null);
        dispatch({ type: 'set_config', apiBaseUrl: defaults.apiBaseUrl, wsBaseUrl: defaults.wsBaseUrl });
        openRoomSocket({
          tableCode: selfSubject.table_code,
          nickname: currentUser.display_name,
          wsBaseUrl: defaults.wsBaseUrl,
          sessionToken: authSession.sessionToken,
          onOpen: (socket) => {
            socket.send(serializeClientMessage(createStartMatchMessage()));
          },
        });
        setStatusMessage('评测已创建，正在进入你的评测牌局。');
      } else {
        setStatusMessage('评测已创建。');
      }
    } catch (error) {
      setStatusMessage(error instanceof Error ? getSocialStatusCopy(error.message) : '创建评测失败。');
    } finally {
      setIsEvaluationSubmitting(false);
    }
  }

  async function handleRefreshEvaluation() {
    await refreshEvaluationSession();
  }

  useEffect(() => {
    if (!authSession?.sessionToken || state.roomSnapshot?.payload.mode !== 'evaluation') {
      return;
    }

    const tableCode = state.roomSnapshot.payload.table_code;
    if (evaluationSession?.subjects.some((subject) => subject.table_code === tableCode)) {
      return;
    }

    let cancelled = false;
    void (async () => {
      try {
        const session = await getEvaluationByTable(defaults.apiBaseUrl, authSession.sessionToken, tableCode);
        if (!cancelled) {
          setEvaluationSession(session);
        }
      } catch (error) {
        if (!cancelled) {
          setStatusMessage(error instanceof Error ? getSocialStatusCopy(error.message) : '加载评测失败。');
        }
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [
    authSession?.sessionToken,
    defaults.apiBaseUrl,
    evaluationSession?.evaluation_id,
    evaluationSession?.subjects,
    state.roomSnapshot?.payload.mode,
    state.roomSnapshot?.payload.table_code,
  ]);

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

    if (actionId === 'start_match') {
      sendMessage(serializeClientMessage(createStartMatchMessage()));
      return;
    }

    if (actionId === 'start_next_round') {
      sendMessage(serializeClientMessage(createStartNextRoundMessage()));
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

  function handlePointGesture(targetSeat: number) {
    sendMessage(serializeClientMessage(createPointGestureMessage(targetSeat)));
  }

  function handleAdjustBots(delta: 1 | -1) {
    sendMessage(serializeClientMessage(createAdjustBotsMessage(delta)));
  }

  function handleSetMinimumHuFan(minimumHuFan: MinimumHuFan) {
    sendMessage(serializeClientMessage(createSetMinimumHuFanMessage(minimumHuFan)));
  }

  function handleSetDealerRepeat(enabled: boolean) {
    sendMessage(serializeClientMessage(createSetDealerRepeatMessage(enabled)));
  }

  function handleSetDealerDouble(enabled: boolean) {
    sendMessage(serializeClientMessage(createSetDealerDoubleMessage(enabled)));
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

  const activePendingInvite = pendingInvites[0] ?? null;
  const pendingInvitePanel = activePendingInvite ? (
    <div className="table-invite-popup" role="dialog" aria-modal="false" aria-label="收到牌局邀请">
      <p>收到 {getInviteCreatorLabel(activePendingInvite, inviteCreatorLabelsByUserId)} 的邀请，是否加入牌局？</p>
      <div className="table-invite-popup__actions">
        <button type="button" onClick={() => handleAcceptInvite(activePendingInvite)}>
          加入
        </button>
        <button type="button" onClick={() => handleRejectInvite(activePendingInvite)}>
          拒绝
        </button>
      </div>
    </div>
  ) : null;

  function handleLeaveTable() {
    if (!socketRef.current || socketRef.current.readyState !== WebSocket.OPEN) {
      handleLeaveToLobby(
        state.roomSnapshot?.payload.table_code ?? state.tableCode,
        '当前连接已断开，正在为你准备新的空牌桌。',
      );
      if (authSession?.sessionToken && currentUser) {
        void createAndEnterEmptyTable(authSession.sessionToken, currentUser.display_name);
      }
      return;
    }

    leavingTableRef.current = true;
    socketRef.current.send(serializeClientMessage(createLeaveTableMessage()));
  }

  function renderBattleScreen() {
    return (
      <Suspense fallback={null}>
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
          isAutoPassKongEnabled={isAutoPassKongEnabled}
          canToggleAutoPassKong={canToggleAutoPassKong}
          onToggleBotTakeover={handleSetBotTakeover}
          onToggleAutoPassKong={setIsAutoPassKongEnabled}
          inviteHumanUsers={inviteHumanUsers}
          inviteAiUsers={inviteAiUsers}
          inviteStatusesByUserId={sentInviteStatusesByUserId}
          pendingInvitePanel={pendingInvitePanel}
          evaluationSession={evaluationSession}
          isEvaluationSubmitting={isEvaluationSubmitting}
          currentUserId={currentUser?.user_id ?? null}
          viewModel={viewModel}
          themeId={themeId}
          themeLabel={getThemeLabel(themeId)}
          onCycleTheme={() => setThemeId((currentThemeId) => getNextThemeId(currentThemeId))}
          onTileSelect={handleTileSelect}
          onTileDoubleClick={handleTileDoubleClick}
          onClaimCandidateSelect={handleClaimCandidateSelect}
          onClaimCandidateActivate={handleClaimCandidateActivate}
          onAction={handleAction}
          onLeaveTable={handleLeaveTable}
          onInvitePlayer={handleInvitePlayer}
          onCreateEvaluation={handleCreateEvaluation}
          onRefreshEvaluation={handleRefreshEvaluation}
          onAddBot={() => handleAdjustBots(1)}
          onRemoveBot={() => handleAdjustBots(-1)}
          onMinimumHuFanChange={handleSetMinimumHuFan}
          onDealerRepeatChange={handleSetDealerRepeat}
          onDealerDoubleChange={handleSetDealerDouble}
          onQuickChat={handleQuickChat}
          onPointGesture={handlePointGesture}
        />
      </Suspense>
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

    return renderBattleScreen();
  }

  return renderBattleScreen();
}
