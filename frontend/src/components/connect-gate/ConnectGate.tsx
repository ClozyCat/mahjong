import { WindowFrame } from '../win98/WindowFrame';

export interface ConnectGateValue {
  apiBaseUrl: string;
  wsBaseUrl: string;
  tableCode: string;
  nickname: string;
  testMode: boolean;
  enforceMinimumEightFan: boolean;
}

interface ConnectGateProps {
  value: ConnectGateValue;
  status: 'idle' | 'connecting' | 'error';
  message?: string | null;
  onChange: (patch: Partial<ConnectGateValue>) => void;
  onCreate: () => void;
  onJoin: () => void;
}

export function ConnectGate({ value, status, message, onChange, onCreate, onJoin }: ConnectGateProps) {
  const disabled = status === 'connecting';
  const statusText = message ?? (disabled ? '正在连接牌桌，请稍候。' : '请输入昵称和牌桌编号后开始。');

  return (
    <section className="connect-gate" aria-label="Room connection setup">
      <WindowFrame title="四风麻将客户端" status={statusText} className="connect-gate__window">
        <div className="connect-gate__panel">
          <p className="connect-gate__eyebrow">联机大厅</p>
          <h1>连接牌桌</h1>

          <label className="connect-gate__field">
            <span>服务地址</span>
            <input
              value={value.apiBaseUrl}
              onChange={(event) => onChange({ apiBaseUrl: event.target.value })}
              disabled={disabled}
              aria-label="服务地址"
            />
          </label>

          <label className="connect-gate__field">
            <span>通信地址</span>
            <input
              value={value.wsBaseUrl}
              onChange={(event) => onChange({ wsBaseUrl: event.target.value })}
              disabled={disabled}
              aria-label="通信地址"
            />
          </label>

          <label className="connect-gate__field">
            <span>牌桌编号</span>
            <input
              value={value.tableCode}
              onChange={(event) => onChange({ tableCode: event.target.value.toUpperCase() })}
              disabled={disabled}
              aria-label="牌桌编号"
            />
          </label>

          <label className="connect-gate__field">
            <span>昵称</span>
            <input
              value={value.nickname}
              onChange={(event) => onChange({ nickname: event.target.value })}
              disabled={disabled}
              aria-label="昵称"
            />
          </label>

          <div className="connect-gate__actions">
            <button
              type="button"
              onClick={() => onChange({ testMode: !value.testMode })}
              disabled={disabled}
              aria-pressed={value.testMode}
            >
              测试模式：{value.testMode ? '开' : '关'}
            </button>
          </div>

          <label className="connect-gate__toggle">
            <input
              type="checkbox"
              checked={value.enforceMinimumEightFan}
              onChange={(event) => onChange({ enforceMinimumEightFan: event.target.checked })}
              disabled={disabled}
            />
            <span>限制八番起胡</span>
          </label>

          <div className="connect-gate__actions">
            <button type="button" onClick={onCreate} disabled={disabled}>
              创建牌桌
            </button>
            <button type="button" onClick={onJoin} disabled={disabled || !value.tableCode || !value.nickname.trim()}>
              加入牌桌
            </button>
          </div>
        </div>
      </WindowFrame>
    </section>
  );
}
