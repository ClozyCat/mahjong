import { startTransition, useEffect, useEffectEvent, useMemo, useReducer, useRef, useState, type MutableRefObject } from 'react';

import { BattleScreen } from './components/battle-screen/BattleScreen';
import { ConnectGate, type ConnectGateValue } from './components/connect-gate/ConnectGate';
import { ApiError, createTable } from './lib/api';
import {
  getActionCandidateGroups,
  getFlowerCandidateTileIds,
  getLocalTurnKongCandidateGroups,
  getLocalTurnKongPromptSignature,
  getMatchingActionGroup,
  shouldAutoPassOpeningFlowers,
} from './lib/kongSelection';
import { createClaimCandidates, createMatchViewModel } from './lib/matchViewModel';
import {
  buildSkillActivationRequest,
  closeSkillActivation,
  createInitialSkillRuntimeState,
  createSkillEnhancedBattleViewModel,
  openSkillActivation,
  syncSkillRuntimeWithSession,
  updateSkillActivationSelection,
} from './lib/skillSystem';
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
  createStartMatchMessage,
  createStartNextRoundMessage,
  parseServerMessage,
  serializeClientMessage,
} from './lib/socket';
import { createInitialSessionState, sessionReducer } from './lib/sessionReducer';
import {
  clearStoredSession,
  loadStoredSession,
  loadStoredThemeId,
  saveStoredSession,
  saveStoredThemeId,
} from './lib/storage';
import { getTableCodeError, normalizeTableCode } from './lib/tableCode';
import { DEFAULT_THEME_ID, getNextThemeId, getRandomThemeId, getThemeLabel, isThemeId } from './lib/themes';
import type { BackendActionType, BattleActionId, ClaimActionId, QuickChatEmoji, SessionState } from './types/match';

const HEARTBEAT_INTERVAL_MS = 20_000;
const LEAVE_TABLE_CONFIRM_MESSAGE = '若主动离开，则无法再次加入对局，是否确定离开牌桌？';
const CLAIM_ACTION_IDS = ['chow', 'pung', 'kong'] as const;

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
  const storedSession = loadStoredSession();

  return {
    defaults,
    storedSession,
  };
}

function getRejectedMessage(reason: string) {
  const lookup: Record<string, string> = {
    table_not_found: '牌桌不存在，请检查牌桌编号后重试。',
    table_full: '牌桌已经坐满了，请换一个牌桌试试。',
    invalid_reconnect_token: '上次的重连凭证已失效，需要重新加入牌桌。',
    seat_occupied: '这个座位已经被占用，请选择其他空位。',
  };

  return lookup[reason] ?? '请求未被服务器接受，请按最新房间状态重试。';
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

function isActionBlockedByOptimisticDiscard(actionId: BattleActionId) {
  return (
    actionId === 'activate_skill' ||
    actionId === 'discard' ||
    actionId === 'flower' ||
    actionId === 'kong' ||
    actionId === 'hu' ||
    actionId === 'chow' ||
    actionId === 'pung' ||
    actionId === 'pass'
  );
}

export default function App() {
  const { defaults, storedSession } = useMemo(getDefaultConfig, []);
  const [themeId, setThemeId] = useState(() => {
    const storedThemeId = loadStoredThemeId();
    const nextThemeId = isThemeId(storedThemeId) ? getRandomThemeId(storedThemeId) : getRandomThemeId();

    return isThemeId(nextThemeId) ? nextThemeId : DEFAULT_THEME_ID;
  });
  const [connectValue, setConnectValue] = useState<ConnectGateValue>({
    tableCode: storedSession?.tableCode ?? '',
    nickname: storedSession?.nickname ?? '',
    tableMode: 'normal',
    enforceMinimumEightFan: true,
  });
  const [statusMessage, setStatusMessage] = useState<string | null>(
    storedSession ? `检测到牌桌 ${storedSession.tableCode} 的历史会话，正在尝试恢复座位。` : null,
  );
  const [state, dispatch] = useReducer(
    sessionReducer,
    undefined,
    (): SessionState => ({
      ...createInitialSessionState(),
      apiBaseUrl: defaults.apiBaseUrl,
      wsBaseUrl: storedSession?.wsBaseUrl ?? defaults.wsBaseUrl,
      tableCode: storedSession?.tableCode ?? '',
      nickname: storedSession?.nickname ?? '',
      reconnectToken: storedSession?.reconnectToken ?? null,
      connectionStatus: storedSession ? 'reconnecting' : 'idle',
    }),
  );
  const socketRef = useRef<WebSocket | null>(null);
  const heartbeatTimerRef = useRef<number | null>(null);
  const sessionRef = useRef(state);
  const leavingTableRef = useRef(false);
  const previousClaimSelectionSignatureRef = useRef<string | null>(null);
  const previousLocalTurnKongPromptSignatureRef = useRef<string | null>(null);
  const previousOpeningFlowerAutoPassSignatureRef = useRef<string | null>(null);
  const previousHadRoomSnapshotRef = useRef(false);
  const [dismissedLocalTurnKongPromptSignature, setDismissedLocalTurnKongPromptSignature] = useState<string | null>(null);
  const [skillRuntime, setSkillRuntime] = useState(createInitialSkillRuntimeState);

  useEffect(() => {
    sessionRef.current = state;
  }, [state]);

  useEffect(() => {
    setSkillRuntime((currentRuntime) => syncSkillRuntimeWithSession(currentRuntime, state));
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
    if (state.reconnectToken && state.tableCode && state.wsBaseUrl) {
      saveStoredSession({
        tableCode: state.tableCode,
        nickname: state.nickname || connectValue.nickname,
        reconnectToken: state.reconnectToken,
        wsBaseUrl: state.wsBaseUrl,
      });
      return;
    }

    clearStoredSession();
  }, [connectValue.nickname, state.nickname, state.reconnectToken, state.tableCode, state.wsBaseUrl]);

  const handleLeaveToLobby = useEffectEvent((tableCode?: string, nextStatusMessage: string | null = null) => {
    leavingTableRef.current = false;
    clearStoredSession();
    dispatch({
      type: 'return_to_lobby',
      tableCode: tableCode ?? sessionRef.current.tableCode,
    });
    closeSocket(socketRef, heartbeatTimerRef);
    setStatusMessage(nextStatusMessage);
  });

  const handleFatalLobbyReset = useEffectEvent((message: string, tableCode?: string) => {
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
      dispatch({ type: 'set_connection_status', status: 'connected' });
      startTransition(() => {
        setConnectValue((current) => ({
          ...current,
          tableCode: message.payload.table_code,
        }));
      });
      setStatusMessage(null);
    }

    dispatch({ type: 'ws_message', message });
  });

  const openRoomSocket = useEffectEvent(
    (options: {
      tableCode: string;
      nickname: string;
      wsBaseUrl: string;
      reconnectToken?: string | null;
      reconnect?: boolean;
    }) => {
      closeSocket(socketRef, heartbeatTimerRef);

      const { tableCode, nickname, wsBaseUrl, reconnectToken, reconnect } = options;
      dispatch({ type: 'set_connection_status', status: reconnect ? 'reconnecting' : 'connecting' });
      dispatch({ type: 'set_credentials', tableCode, nickname });
      dispatch({ type: 'set_config', wsBaseUrl });

      const socket = new WebSocket(buildWebSocketUrl(wsBaseUrl, tableCode));
      socketRef.current = socket;

      socket.onopen = () => {
        const message =
          reconnect && reconnectToken
            ? createReconnectMessage(reconnectToken)
            : createJoinTableMessage(nickname);
        socket.send(serializeClientMessage(message));

        heartbeatTimerRef.current = window.setInterval(() => {
          if (socket.readyState === WebSocket.OPEN) {
            socket.send(serializeClientMessage(createHeartbeatMessage(new Date().toISOString())));
          }
        }, HEARTBEAT_INTERVAL_MS);
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
        if (current.reconnectToken && current.tableCode && current.wsBaseUrl) {
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
        nickname: state.nickname || connectValue.nickname,
        wsBaseUrl,
        reconnectToken: state.reconnectToken,
        reconnect: true,
      });
    }, 1000);

    return () => {
      window.clearTimeout(timeoutId);
    };
  }, [connectValue.nickname, openRoomSocket, state.connectionStatus, state.nickname, state.reconnectToken, state.tableCode, state.wsBaseUrl]);

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

  useEffect(() => {
    const pendingAction = state.roomSnapshot?.payload.private_state?.pending_action;
    const privateState = state.roomSnapshot?.payload.private_state;
    const autoPassSignature =
      shouldAutoPassOpeningFlowers(state) &&
      pendingAction?.type === 'opening_flowers' &&
      typeof pendingAction.seat_index === 'number'
        ? `${privateState?.round_id ?? ''}:${pendingAction.seat_index}:${pendingAction.deadline_at}`
        : null;

    if (
      autoPassSignature &&
      autoPassSignature !== previousOpeningFlowerAutoPassSignatureRef.current &&
      sendMessage(serializeClientMessage(createActionRequestMessage('pass')))
    ) {
      dispatch({ type: 'set_selected_tiles', tileIds: [], mode: null });
    }

    previousOpeningFlowerAutoPassSignatureRef.current = autoPassSignature;
  }, [state]);

  const localTurnKongPromptSignature = getLocalTurnKongPromptSignature(state);
  const localTurnKongCandidateGroups = getLocalTurnKongCandidateGroups(state);
  const hasLocalTurnKongPrompt =
    localTurnKongPromptSignature !== null && localTurnKongPromptSignature !== dismissedLocalTurnKongPromptSignature;
  const normalizedRequestedTableCode = normalizeTableCode(connectValue.tableCode);
  const tableCodeError = getTableCodeError(connectValue.tableCode);
  const hasNickname = connectValue.nickname.trim().length > 0;
  const canCreate =
    state.connectionStatus !== 'connecting' &&
    state.connectionStatus !== 'reconnecting' &&
    hasNickname &&
    tableCodeError === null;
  const canJoin =
    state.connectionStatus !== 'connecting' &&
    state.connectionStatus !== 'reconnecting' &&
    hasNickname &&
    normalizedRequestedTableCode.length > 0 &&
    tableCodeError === null;

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

  function sendMessage(message: string) {
    if (!socketRef.current || socketRef.current.readyState !== WebSocket.OPEN) {
      setStatusMessage('当前尚未建立连接。');
      return false;
    }

    socketRef.current.send(message);
    return true;
  }

  async function handleCreate() {
    if (!connectValue.nickname.trim()) {
      setStatusMessage('请先输入昵称，再创建牌桌。');
      return;
    }

    if (tableCodeError) {
      return;
    }

    try {
      setStatusMessage('正在创建牌桌...');
      dispatch({ type: 'set_config', apiBaseUrl: defaults.apiBaseUrl, wsBaseUrl: defaults.wsBaseUrl });
      const requestedTableCode = normalizedRequestedTableCode;
      const table = await createTable(
        defaults.apiBaseUrl,
        requestedTableCode || undefined,
        connectValue.tableMode,
        connectValue.enforceMinimumEightFan,
      );
      startTransition(() => {
        setConnectValue((current) => ({
          ...current,
          tableCode: table.table_code,
        }));
      });
      openRoomSocket({
        tableCode: table.table_code,
        nickname: connectValue.nickname.trim(),
        wsBaseUrl: defaults.wsBaseUrl,
      });
    } catch (error) {
      if (
        error instanceof ApiError &&
        error.status === 409 &&
        typeof error.detail === 'object' &&
        error.detail !== null &&
        'detail' in error.detail &&
        (error.detail as { detail: unknown }).detail === 'table_code_exists'
      ) {
        const requestedTableCode = normalizedRequestedTableCode;
        const shouldJoin = window.confirm(`牌桌编号 ${requestedTableCode} 已存在，是否直接加入该牌桌？`);
        if (shouldJoin) {
          handleJoin();
          return;
        }
        setStatusMessage(`牌桌编号 ${requestedTableCode} 已存在，请更换编号或直接加入。`);
        return;
      }

      setStatusMessage(error instanceof Error ? error.message : '创建牌桌失败。');
      dispatch({ type: 'set_connection_status', status: 'error' });
    }
  }

  function handleJoin() {
    if (!connectValue.tableCode.trim() || !connectValue.nickname.trim()) {
      setStatusMessage('加入牌桌前请先填写牌桌编号和昵称。');
      return;
    }

    if (tableCodeError) {
      return;
    }

    setStatusMessage('正在加入牌桌...');
    dispatch({ type: 'set_config', apiBaseUrl: defaults.apiBaseUrl, wsBaseUrl: defaults.wsBaseUrl });
    openRoomSocket({
      tableCode: normalizedRequestedTableCode,
      nickname: connectValue.nickname.trim(),
      wsBaseUrl: defaults.wsBaseUrl,
    });
  }

  function handleTileSelect(tileId: string) {
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
    if (state.optimisticDiscard && isActionBlockedByOptimisticDiscard(actionId)) {
      return;
    }

    if (actionId === 'activate_skill') {
      setSkillRuntime((currentRuntime) => openSkillActivation(currentRuntime, state));
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

      dispatch({ type: 'queue_optimistic_discard', tileId: discardTileId });
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
      if (hasLocalTurnKongPrompt && localTurnKongPromptSignature) {
        setDismissedLocalTurnKongPromptSignature(localTurnKongPromptSignature);
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
    dispatch({
      type: 'set_selected_tiles',
      tileIds,
      mode: actionId,
    });
  }

  function handleClaimCandidateActivate(actionId: ClaimActionId, tileIds: string[]) {
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

  function handleTileDoubleClick(tileId: string) {
    if (!canQuickDiscard(state, hasLocalTurnKongPrompt)) {
      return;
    }

    if (!sendMessage(serializeClientMessage(createActionRequestMessage('discard', [tileId])))) {
      return;
    }

    dispatch({ type: 'queue_optimistic_discard', tileId });
    dispatch({ type: 'set_selected_tiles', tileIds: [], mode: null });
  }

  function handleCopyTableCode() {
    if (!state.tableCode && !connectValue.tableCode) {
      return;
    }

    const tableCode = state.tableCode || connectValue.tableCode;
    if (navigator.clipboard?.writeText) {
      navigator.clipboard.writeText(tableCode).catch(() => undefined);
    }
    setStatusMessage(`已复制牌桌编号 ${tableCode}。`);
  }

  function handleLeaveTable() {
    const shouldConfirmLeave = state.roomSnapshot?.payload.phase !== 'waiting';
    if (shouldConfirmLeave && !window.confirm(LEAVE_TABLE_CONFIRM_MESSAGE)) {
      return;
    }

    if (!socketRef.current || socketRef.current.readyState !== WebSocket.OPEN) {
      handleLeaveToLobby(
        state.roomSnapshot?.payload.table_code ?? state.tableCode,
        '当前连接已断开，已返回大厅。若仍需进入该牌桌，可使用牌桌编号重新加入。',
      );
      return;
    }

    leavingTableRef.current = true;
    socketRef.current.send(serializeClientMessage(createLeaveTableMessage()));
  }

  const viewModel = createMatchViewModel(state, {
    showLocalTurnKongPrompt: hasLocalTurnKongPrompt,
  });
  const skillEnhancedViewModel = createSkillEnhancedBattleViewModel(viewModel, state, skillRuntime);

  if (!state.roomSnapshot) {
    return (
      <ConnectGate
        value={connectValue}
        status={state.connectionStatus === 'connecting' || state.connectionStatus === 'reconnecting' ? 'connecting' : statusMessage ? 'error' : 'idle'}
        message={statusMessage}
        themeLabel={getThemeLabel(themeId)}
        tableCodeError={tableCodeError}
        canCreate={canCreate}
        canJoin={canJoin}
        onChange={(patch) => {
          startTransition(() => {
            setConnectValue((current) => ({ ...current, ...patch }));
          });
        }}
        onCreate={handleCreate}
        onJoin={handleJoin}
      />
    );
  }

  return (
    <BattleScreen
      viewModel={skillEnhancedViewModel}
      themeId={themeId}
      themeLabel={getThemeLabel(themeId)}
      onCycleTheme={() => setThemeId((currentThemeId) => getNextThemeId(currentThemeId))}
      onTileSelect={handleTileSelect}
      onTileDoubleClick={handleTileDoubleClick}
      onClaimCandidateSelect={handleClaimCandidateSelect}
      onClaimCandidateActivate={handleClaimCandidateActivate}
      onAction={handleAction}
      onSkillSelect={(skillId) => {
        sendMessage(serializeClientMessage(createActionRequestMessage('select_skill', [skillId])));
      }}
      onSkillDecline={() => {
        sendMessage(serializeClientMessage(createActionRequestMessage('decline_skill')));
      }}
      onCloseSkillActivation={() => setSkillRuntime((currentRuntime) => closeSkillActivation(currentRuntime))}
      onConfirmSkillActivation={() => {
        const request = buildSkillActivationRequest(skillRuntime);
        if (!request) {
          return;
        }

        if (!sendMessage(serializeClientMessage(createActionRequestMessage(request.actionType, request.tileIds)))) {
          return;
        }

        setSkillRuntime((currentRuntime) => closeSkillActivation(currentRuntime));
      }}
      onSkillActivationTargetSelect={(seatIndex) =>
        setSkillRuntime((currentRuntime) =>
          updateSkillActivationSelection(currentRuntime, {
            selectedTargetSeat: seatIndex,
          }),
        )
      }
      onSkillActivationTileSelect={(tileId) =>
        setSkillRuntime((currentRuntime) =>
          updateSkillActivationSelection(currentRuntime, {
            selectedTileId: tileId,
          }),
        )
      }
      onSkillActivationMeldSelect={(meldIndex) =>
        setSkillRuntime((currentRuntime) =>
          updateSkillActivationSelection(currentRuntime, {
            selectedMeldIndex: meldIndex,
          }),
        )
      }
      onCopyTableCode={handleCopyTableCode}
      onLeaveTable={handleLeaveTable}
      onAddBot={() => handleAdjustBots(1)}
      onRemoveBot={() => handleAdjustBots(-1)}
      onQuickChat={handleQuickChat}
    />
  );
}
