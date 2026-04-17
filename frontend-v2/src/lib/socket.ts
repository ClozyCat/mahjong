import type { ClientMessage, ServerMessage } from "../types/protocol";

export type SocketState =
  | "idle"
  | "connecting"
  | "open"
  | "closed"
  | "error";

export interface SocketHandlers {
  onOpen?: () => void;
  onMessage: (msg: ServerMessage) => void;
  onClose?: (ev: CloseEvent) => void;
  onError?: (ev: Event) => void;
  onStateChange?: (state: SocketState) => void;
}

const HEARTBEAT_INTERVAL_MS = 20_000;

export class MahjongSocket {
  private ws: WebSocket | null = null;
  private heartbeatTimer: number | null = null;
  private handlers: SocketHandlers;
  private url: string;
  private state: SocketState = "idle";

  constructor(url: string, handlers: SocketHandlers) {
    this.url = url;
    this.handlers = handlers;
  }

  get readyState(): SocketState {
    return this.state;
  }

  connect(): void {
    if (this.ws) return;
    this.setState("connecting");
    try {
      this.ws = new WebSocket(this.url);
    } catch (err) {
      console.error("ws create failed", err);
      this.setState("error");
      return;
    }
    this.ws.onopen = () => {
      this.setState("open");
      this.startHeartbeat();
      this.handlers.onOpen?.();
    };
    this.ws.onmessage = (ev) => {
      try {
        const msg = JSON.parse(ev.data) as ServerMessage;
        this.handlers.onMessage(msg);
      } catch (err) {
        console.warn("bad ws payload", err, ev.data);
      }
    };
    this.ws.onclose = (ev) => {
      this.cleanup();
      this.setState("closed");
      this.handlers.onClose?.(ev);
    };
    this.ws.onerror = (ev) => {
      this.handlers.onError?.(ev);
    };
  }

  send(msg: ClientMessage): boolean {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) return false;
    this.ws.send(JSON.stringify(msg));
    return true;
  }

  close(): void {
    this.cleanup();
    if (this.ws) {
      try {
        this.ws.close();
      } catch {
        /* ignore */
      }
      this.ws = null;
    }
    this.setState("closed");
  }

  private setState(state: SocketState) {
    if (this.state !== state) {
      this.state = state;
      this.handlers.onStateChange?.(state);
    }
  }

  private startHeartbeat() {
    this.stopHeartbeat();
    this.heartbeatTimer = window.setInterval(() => {
      this.send({
        type: "heartbeat",
        payload: { sent_at: new Date().toISOString() },
      });
    }, HEARTBEAT_INTERVAL_MS);
  }

  private stopHeartbeat() {
    if (this.heartbeatTimer !== null) {
      window.clearInterval(this.heartbeatTimer);
      this.heartbeatTimer = null;
    }
  }

  private cleanup() {
    this.stopHeartbeat();
  }
}

export function buildWsUrl(wsBase: string, tableCode: string): string {
  const base = wsBase.replace(/\/$/, "");
  return `${base}/ws/${encodeURIComponent(tableCode)}`;
}
