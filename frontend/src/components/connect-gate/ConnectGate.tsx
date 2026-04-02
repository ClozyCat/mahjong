import { useEffect, useRef, useState } from 'react';

import { WindowFrame } from '../win10/WindowFrame';

export interface ConnectGateValue {
  tableCode: string;
  nickname: string;
  testMode: boolean;
  enforceMinimumEightFan: boolean;
}

interface ConnectGateProps {
  value: ConnectGateValue;
  status: 'idle' | 'connecting' | 'error';
  themeLabel: string;
  tableCodeError?: string | null;
  canCreate: boolean;
  canJoin: boolean;
  message?: string | null;
  onChange: (patch: Partial<ConnectGateValue>) => void;
  onCreate: () => void;
  onJoin: () => void;
}

const CONNECT_GATE_QUOTES = [
  '牌局未开，先定心气。',
  '落子无悔，起手有光。',
  '风起四座，静待良局。',
  '牌有千变，心守一线。',
  '好局不怕晚，妙手自会来。',
  '入席先安神，出手见真章。',
];

export function ConnectGate({
  value,
  status,
  themeLabel,
  tableCodeError,
  canCreate,
  canJoin,
  message,
  onChange,
  onCreate,
  onJoin,
}: ConnectGateProps) {
  const [footnoteQuote] = useState(
    () => CONNECT_GATE_QUOTES[Math.floor(Math.random() * CONNECT_GATE_QUOTES.length)],
  );
  const [tableCodeDraft, setTableCodeDraft] = useState(value.tableCode);
  const [nicknameDraft, setNicknameDraft] = useState(value.nickname);
  const tableCodeComposingRef = useRef(false);
  const nicknameComposingRef = useRef(false);
  const disabled = status === 'connecting';
  const helperText = tableCodeError ?? '支持 1-12 位数字或英文字母；留空创建时将自动分配。';
  const statusText =
    message ??
    tableCodeError ??
    (disabled ? '正在连接牌桌，请稍候。' : '输入昵称后即可创建牌桌，或填写编号加入现有牌局。');

  useEffect(() => {
    if (!tableCodeComposingRef.current) {
      setTableCodeDraft(value.tableCode);
    }
  }, [value.tableCode]);

  useEffect(() => {
    if (!nicknameComposingRef.current) {
      setNicknameDraft(value.nickname);
    }
  }, [value.nickname]);

  function commitTableCode(nextValue: string) {
    const normalizedValue = nextValue.toUpperCase();
    setTableCodeDraft(normalizedValue);
    onChange({ tableCode: normalizedValue });
  }

  function commitNickname(nextValue: string) {
    setNicknameDraft(nextValue);
    onChange({ nickname: nextValue });
  }

  return (
    <section className="connect-gate" aria-label="Room connection setup">
      <WindowFrame title="四风麻将客户端" status={statusText} className="connect-gate__window">
        <div className="connect-gate__panel">
          <div className="connect-gate__shell">
            <div className="connect-gate__hero">
              <p className="connect-gate__eyebrow">联机大厅</p>
              <h1>启局入席</h1>
              <p className="connect-gate__lead">
                大厅会在每次开启时随机取用一套中国色。保留当前牌桌内的沉浸感，也让每次入局都有一点新鲜气息。
              </p>

              <div className="connect-gate__meta">
                <article className="connect-gate__meta-card connect-gate__meta-card--palette">
                  <div className="connect-gate__meta-heading">
                    <span>当前配色</span>
                    <em>入厅时随机换新</em>
                  </div>
                  <strong>{themeLabel}</strong>
                  <div className="connect-gate__palette-preview" aria-hidden="true">
                    <span />
                    <span />
                    <span />
                  </div>
                </article>
              </div>
            </div>

            <div className="connect-gate__form">
              <label className="connect-gate__field">
                <span>牌桌编号</span>
                <input
                  value={tableCodeDraft}
                  onChange={(event) => {
                    const nextValue = event.target.value;
                    setTableCodeDraft(nextValue);

                    if (!tableCodeComposingRef.current) {
                      commitTableCode(nextValue);
                    }
                  }}
                  onCompositionStart={() => {
                    tableCodeComposingRef.current = true;
                  }}
                  onCompositionEnd={(event) => {
                    tableCodeComposingRef.current = false;
                    commitTableCode(event.currentTarget.value);
                  }}
                  disabled={disabled}
                  maxLength={12}
                  autoCapitalize="characters"
                  spellCheck={false}
                  aria-label="牌桌编号"
                  aria-invalid={tableCodeError ? 'true' : 'false'}
                  aria-describedby="connect-gate-table-code-hint"
                />
                <small
                  id="connect-gate-table-code-hint"
                  className={tableCodeError ? 'connect-gate__hint connect-gate__hint--error' : 'connect-gate__hint'}
                >
                  {helperText}
                </small>
              </label>

              <label className="connect-gate__field">
                <span>昵称</span>
                <input
                  value={nicknameDraft}
                  onChange={(event) => {
                    const nextValue = event.target.value;
                    setNicknameDraft(nextValue);

                    if (!nicknameComposingRef.current) {
                      commitNickname(nextValue);
                    }
                  }}
                  onCompositionStart={() => {
                    nicknameComposingRef.current = true;
                  }}
                  onCompositionEnd={(event) => {
                    nicknameComposingRef.current = false;
                    commitNickname(event.currentTarget.value);
                  }}
                  disabled={disabled}
                  aria-label="昵称"
                />
              </label>

              <div className="connect-gate__actions connect-gate__actions--toggles">
                <button
                  type="button"
                  className="connect-gate__toggle"
                  onClick={() => onChange({ testMode: !value.testMode })}
                  disabled={disabled}
                  aria-pressed={value.testMode}
                >
                  <span>测试模式</span>
                  <strong>{value.testMode ? '开启' : '关闭'}</strong>
                </button>
                <button
                  type="button"
                  className="connect-gate__toggle"
                  onClick={() => onChange({ enforceMinimumEightFan: !value.enforceMinimumEightFan })}
                  disabled={disabled}
                  aria-pressed={value.enforceMinimumEightFan}
                >
                  <span>八番起胡</span>
                  <strong>{value.enforceMinimumEightFan ? '限制中' : '已放宽'}</strong>
                </button>
              </div>

              <div className="connect-gate__actions connect-gate__actions--primary">
                <button type="button" className="connect-gate__button connect-gate__button--primary" onClick={onCreate} disabled={!canCreate}>
                  创建牌桌
                </button>
                <button type="button" className="connect-gate__button connect-gate__button--secondary" onClick={onJoin} disabled={!canJoin}>
                  加入牌桌
                </button>
              </div>

              <p className="connect-gate__footnote">{footnoteQuote}</p>
            </div>
          </div>
        </div>
      </WindowFrame>
    </section>
  );
}
