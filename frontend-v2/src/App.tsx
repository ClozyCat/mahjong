import { useCallback, useEffect, useMemo, useReducer, useRef, useState } from "react";
import { Lobby } from "./components/Lobby";
import { WaitingRoom } from "./components/WaitingRoom";
import { TableStage } from "./components/TableStage";
import { SettlementOverlay } from "./components/SettlementOverlay";
import { SkillDraftOverlay } from "./components/SkillDraftOverlay";
import { EntranceTransition } from "./components/EntranceTransition";
import { ToastLayer } from "./components/ToastLayer";
import { INITIAL_STATE, reduce } from "./lib/session";
import {
  clearSession,
  loadSession,
  saveSession,
} from "./lib/storage";
import { MahjongSocket, buildWsUrl } from "./lib/socket";
import type {
  ClientMessage,
  RoundEvent,
  ServerMessage,
  SkillOption,
} from "./types/protocol";
import type { ActionDockEmit } from "./components/ActionDock";
import { isFlowerKey } from "./lib/tileUtils";
import { getWsBaseUrl } from "./lib/env";

const RECONNECT_DELAY_MS = 1000;
const MAX_RECONNECT_ATTEMPTS = 3;

export function App() {
  const [state, dispatch] = useReducer(reduce, INITIAL_STATE);
  const socketRef = useRef<MahjongSocket | null>(null);
  const dispatchRef = useRef<typeof dispatch>(dispatch);
  dispatchRef.current = dispatch;

  const [selectedTileId, setSelectedTileId] = useState<string | null>(null);
  const [entranceKey, setEntranceKey] = useState<string | null>(null);
  const [lastRoundId, setLastRoundId] = useState<string | null>(null);

  // 进入牌桌:建立 socket
  const openSocket = useCallback(
    (opts: {
      tableCode: string;
      nickname: string;
      wsBaseUrl: string;
      reconnectToken: string | null;
      kind: "fresh" | "reconnect";
    }) => {
      const wsUrl = buildWsUrl(opts.wsBaseUrl, opts.tableCode);
      const socket = new MahjongSocket(wsUrl, {
        onOpen: () => {
          if (opts.kind === "reconnect" && opts.reconnectToken) {
            socket.send({
              type: "reconnect",
              payload: { reconnect_token: opts.reconnectToken },
            });
          } else {
            socket.send({
              type: "join_table",
              payload: { nickname: opts.nickname },
            });
          }
        },
        onMessage: (msg) => handleServerMessage(msg),
        onClose: () => {
          dispatchRef.current({ type: "socket_closed" });
        },
        onError: () => {
          /* noop:onClose 会跟着触发 */
        },
      });
      socketRef.current = socket;
      socket.connect();
    },
    [],
  );

  const handleServerMessage = useCallback((msg: ServerMessage) => {
    const d = dispatchRef.current;
    switch (msg.type) {
      case "room_snapshot":
        d({ type: "apply_snapshot", snapshot: msg.payload });
        break;
      case "action_prompt":
        d({ type: "apply_prompt", prompt: msg.payload });
        break;
      case "match_result":
        d({ type: "apply_match_result", result: msg.payload });
        break;
      case "round_event":
        d({
          type: "apply_round_event",
          event: msg.payload.event as RoundEvent,
        });
        break;
      case "player_presence":
        d({ type: "apply_presence", data: msg.payload });
        break;
      case "quick_chat":
        d({ type: "apply_quick_chat", data: msg.payload });
        break;
      case "leave_table_accepted":
        clearSession();
        d({ type: "reset_to_lobby" });
        break;
      case "action_rejected":
        d({ type: "apply_action_rejected", reason: msg.payload.reason });
        break;
      case "heartbeat":
        break;
      default:
        break;
    }
  }, []);

  // 初始:尝试恢复会话
  useEffect(() => {
    const session = loadSession();
    if (session && session.reconnectToken) {
      dispatch({
        type: "begin_connect",
        tableCode: session.tableCode,
        nickname: session.nickname,
        wsBaseUrl: session.wsBaseUrl || getWsBaseUrl(),
      });
      openSocket({
        tableCode: session.tableCode,
        nickname: session.nickname,
        wsBaseUrl: session.wsBaseUrl || getWsBaseUrl(),
        reconnectToken: session.reconnectToken,
        kind: "reconnect",
      });
    }
  }, [openSocket]);

  // 持久化 session
  useEffect(() => {
    if (
      state.tableCode &&
      state.nickname &&
      state.reconnectToken !== undefined
    ) {
      saveSession({
        tableCode: state.tableCode,
        nickname: state.nickname,
        reconnectToken: state.reconnectToken,
        wsBaseUrl: state.wsBaseUrl,
      });
    }
  }, [state.tableCode, state.nickname, state.reconnectToken, state.wsBaseUrl]);

  // 自动重连
  useEffect(() => {
    if (state.view !== "reconnecting") return;
    if (!state.tableCode) return;
    if (state.reconnectAttempts >= MAX_RECONNECT_ATTEMPTS) {
      clearSession();
      dispatch({ type: "reset_to_lobby", error: "连接丢失,已返回大厅" });
      return;
    }
    const timer = window.setTimeout(() => {
      dispatch({ type: "bump_reconnect_attempts" });
      openSocket({
        tableCode: state.tableCode!,
        nickname: state.nickname,
        wsBaseUrl: state.wsBaseUrl || getWsBaseUrl(),
        reconnectToken: state.reconnectToken,
        kind: state.reconnectToken ? "reconnect" : "fresh",
      });
    }, RECONNECT_DELAY_MS);
    return () => window.clearTimeout(timer);
  }, [state.view, state.reconnectAttempts, state.reconnectToken, state.tableCode, state.nickname, state.wsBaseUrl, openSocket]);

  // 进入房间时的进场动画 key
  useEffect(() => {
    const snap = state.snapshot;
    if (!snap || snap.phase !== "playing") return;
    const rid = snap.private_state?.round_id;
    if (rid && rid !== lastRoundId) {
      setLastRoundId(rid);
      setEntranceKey(rid);
    }
  }, [state.snapshot, lastRoundId]);

  // 自动过补花
  useEffect(() => {
    const snap = state.snapshot;
    const priv = snap?.private_state;
    const pending = priv?.pending_action;
    if (!snap || !priv || !pending) return;
    if (pending.type !== "opening_flowers") return;
    if (pending.seat_index !== snap.local_seat) return;
    const me = priv.players.find((p) => p.seat_index === snap.local_seat);
    if (!me?.concealed_tiles) return;
    const hasFlower = me.concealed_tiles.some((t) => isFlowerKey(t.tile_key));
    if (!hasFlower) {
      sendClient({ type: "action_request", payload: { action_type: "pass", tile_ids: [] } });
    }
  }, [state.snapshot]);

  // 检测到本家多种杠牌候选时,弹本地提示逻辑由 TableStage 自行根据候选数决定

  const sendClient = useCallback((msg: ClientMessage) => {
    socketRef.current?.send(msg);
  }, []);

  const handleConnectFromLobby = useCallback(
    (args: { tableCode: string; nickname: string; wsBaseUrl: string }) => {
      dispatch({
        type: "begin_connect",
        tableCode: args.tableCode,
        nickname: args.nickname,
        wsBaseUrl: args.wsBaseUrl,
      });
      openSocket({
        tableCode: args.tableCode,
        nickname: args.nickname,
        wsBaseUrl: args.wsBaseUrl,
        reconnectToken: null,
        kind: "fresh",
      });
    },
    [openSocket],
  );

  const handleLeave = useCallback(() => {
    const snap = state.snapshot;
    if (snap && (snap.phase === "playing" || snap.phase === "settlement")) {
      const ok = window.confirm("当前对局尚未结束,离桌后座位将由 bot 接管,确定离开?");
      if (!ok) return;
    }
    sendClient({ type: "leave_table", payload: {} });
  }, [sendClient, state.snapshot]);

  const handleReady = useCallback(
    (ready: boolean) => {
      sendClient({ type: "ready", payload: { ready } });
    },
    [sendClient],
  );

  const handleAdjustBots = useCallback(
    (delta: 1 | -1) => {
      sendClient({ type: "adjust_bots", payload: { delta } });
    },
    [sendClient],
  );

  const handleStartMatch = useCallback(() => {
    sendClient({ type: "start_match", payload: {} });
  }, [sendClient]);

  const handleEmit = useCallback(
    (emit: ActionDockEmit) => {
      if (emit.action_type === "discard" && emit.tile_ids.length === 1) {
        dispatch({
          type: "set_optimistic_discard",
          tileId: emit.tile_ids[0],
        });
      }
      if (emit.action_type === "flower" && emit.tile_ids.length === 1) {
        dispatch({
          type: "set_optimistic_flower",
          tileId: emit.tile_ids[0],
        });
      }
      sendClient({
        type: "action_request",
        payload: {
          action_type: emit.action_type,
          tile_ids: emit.tile_ids,
        },
      });
    },
    [sendClient],
  );

  const handleContinue = useCallback(() => {
    sendClient({ type: "start_next_round", payload: {} });
  }, [sendClient]);

  const handleRestart = useCallback(() => {
    sendClient({ type: "restart_match", payload: {} });
  }, [sendClient]);

  const handleChatSend = useCallback(
    (target: number, emoji: string) => {
      sendClient({
        type: "quick_chat",
        payload: { target_seat: target, emoji },
      });
    },
    [sendClient],
  );

  const handleSkillSelect = useCallback(
    (opt: SkillOption) => {
      sendClient({
        type: "action_request",
        payload: {
          action_type: "select_skill",
          tile_ids: [opt.skill_id],
        },
      });
    },
    [sendClient],
  );

  const handleSkillDecline = useCallback(() => {
    sendClient({
      type: "action_request",
      payload: { action_type: "decline_skill", tile_ids: [] },
    });
  }, [sendClient]);

  const dismissToast = useCallback((id: string) => {
    dispatch({ type: "dismiss_toast", id });
  }, []);

  const clearChat = useCallback((messageId: string) => {
    dispatch({ type: "clear_chat_bubble", messageId });
  }, []);

  // 进入/离开 socket 的清理
  useEffect(() => {
    return () => {
      socketRef.current?.close();
    };
  }, []);

  const view = useMemo(() => {
    if (state.view === "lobby" || state.view === "error") {
      return <Lobby onConnect={handleConnectFromLobby} initialError={state.error} />;
    }

    if (state.view === "connecting" || (state.view === "reconnecting" && !state.snapshot)) {
      return (
        <div className="overlay">
          <div className="spinner" />
          <div className="muted">{state.view === "reconnecting" ? "重连中" : "连接中"}</div>
        </div>
      );
    }

    if (!state.snapshot) {
      return (
        <div className="overlay">
          <div className="spinner" />
          <div className="muted">同步中</div>
        </div>
      );
    }

    const snap = state.snapshot;

    if (snap.phase === "waiting") {
      return (
        <WaitingRoom
          snapshot={snap}
          onReady={handleReady}
          onAdjustBots={handleAdjustBots}
          onStart={handleStartMatch}
          onLeave={handleLeave}
        />
      );
    }

    const pending = snap.private_state?.pending_action ?? null;
    const myPending =
      pending && "seat_index" in pending && pending.seat_index === snap.local_seat
        ? pending
        : pending && pending.type === "claim_window"
          ? pending
          : pending && pending.type === "rob_kong_window"
            ? pending
            : null;

    return (
      <>
        <TableStage
          snapshot={snap}
          pending={myPending}
          selectedTileId={selectedTileId}
          onSelectTile={setSelectedTileId}
          onEmitAction={handleEmit}
          optimisticDiscardId={state.optimisticDiscardTileId}
          promptDeadline={state.prompt?.deadline_at ?? null}
          chatBubbles={state.chatBubbles}
          onChatSend={handleChatSend}
          onChatExpire={clearChat}
          onLeave={handleLeave}
        />
        {(snap.phase === "settlement" || snap.phase === "finished") && (
          <SettlementOverlay
            snapshot={snap}
            result={state.matchResult}
            onContinue={handleContinue}
            onRestart={handleRestart}
            onLeave={handleLeave}
          />
        )}
        {snap.private_state?.skill_draft ? (
          <SkillDraftOverlay
            draft={snap.private_state.skill_draft}
            onSelect={handleSkillSelect}
            onDecline={handleSkillDecline}
          />
        ) : null}
        {snap.phase === "playing" && entranceKey ? (
          <EntranceTransition
            keyMarker={entranceKey}
            title={`${windCn(snap.match_state?.prevailing_wind ?? "east")}風局`}
            subtitle={`第 ${snap.match_state?.hand_number ?? 1} 场 · ${codeFmt(snap.table_code)}`}
            onDone={() => setEntranceKey(null)}
          />
        ) : null}
      </>
    );
  }, [
    state,
    selectedTileId,
    handleConnectFromLobby,
    handleReady,
    handleAdjustBots,
    handleStartMatch,
    handleLeave,
    handleEmit,
    handleContinue,
    handleRestart,
    handleChatSend,
    handleSkillSelect,
    handleSkillDecline,
    clearChat,
    entranceKey,
  ]);

  return (
    <div className="app-root">
      {view}
      <ToastLayer toasts={state.toasts} onDismiss={dismissToast} />
    </div>
  );
}

function windCn(w: string) {
  return { east: "東", south: "南", west: "西", north: "北" }[w] ?? w;
}
function codeFmt(c: string) {
  return c.split("").join(" ");
}
