import { useEffect, useRef, useState } from 'react';

export interface ConnectGateValue {
  tableCode: string;
  nickname: string;
}

interface ConnectGateProps {
  value: ConnectGateValue;
  status: 'idle' | 'connecting' | 'error';
  themeLabel: string;
  tableCodeError?: string | null;
  canCreate: boolean;
  canJoin: boolean;
  canWatch?: boolean;
  message?: string | null;
  onChange: (patch: Partial<ConnectGateValue>) => void;
  onCreate: () => void;
  onJoin: () => void;
  onWatch?: () => void;
}
export function ConnectGate({
  value,
  status,
  themeLabel,
  tableCodeError,
  canCreate,
  canJoin,
  canWatch,
  message,
  onChange,
  onCreate,
  onJoin,
  onWatch,
}: ConnectGateProps) {
  const [tableCodeDraft, setTableCodeDraft] = useState(value.tableCode);
  const [nicknameDraft, setNicknameDraft] = useState(value.nickname);
  const tableCodeComposingRef = useRef(false);
  const nicknameComposingRef = useRef(false);
  const disabled = status === 'connecting';
  const helperText = tableCodeError ?? '留空创建时将自动分配。';
  const statusText =
    message ??
    tableCodeError ??
    (disabled ? '正在建立连接...' : '输入昵称后即可开启牌局。');

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
    <main className="connect-gate" aria-label="Lobby">
      <div className="connect-gate__backdrop" aria-hidden="true" />

      <button
        type="button"
        className="connect-gate__admin-link"
        onClick={() => { window.location.href = 'https://49.233.186.248/ca'; }}
        title="前往谜格"
      >
        MIGE
      </button>

      <div className="connect-gate__card">
        <header className="connect-gate__header">
          <div className="connect-gate__brand">
            <span className="connect-gate__eyebrow">Online Lobby</span>
            <h1>启局入席</h1>
            <div className="connect-gate__status" role="status">
              <span className={`connect-gate__status-dot connect-gate__status-dot--${status}`} />
              {statusText}
            </div>
          </div>

          <div className="connect-gate__theme-badge">
            <span>{themeLabel}</span>
          </div>
        </header>

        <section className="connect-gate__body">
          <div className="connect-gate__inputs">
            <div className="connect-gate__field">
              <label htmlFor="table-code">牌桌编号</label>
              <div className="connect-gate__input-wrapper">
                <input
                  id="table-code"
                  value={tableCodeDraft}
                  placeholder="AUTO"
                  onChange={(event) => {
                    const nextValue = event.target.value;
                    setTableCodeDraft(nextValue);
                    if (!tableCodeComposingRef.current) commitTableCode(nextValue);
                  }}
                  onCompositionStart={() => { tableCodeComposingRef.current = true; }}
                  onCompositionEnd={(event) => {
                    tableCodeComposingRef.current = false;
                    commitTableCode(event.currentTarget.value);
                  }}
                  disabled={disabled}
                  maxLength={12}
                  autoCapitalize="characters"
                  spellCheck={false}
                />
                <small className={tableCodeError ? 'error' : ''}>{helperText}</small>
              </div>
            </div>

            <div className="connect-gate__field">
              <label htmlFor="nickname">您的昵称</label>
              <div className="connect-gate__input-wrapper">
                <input
                  id="nickname"
                  value={nicknameDraft}
                  placeholder="请输入..."
                  onChange={(event) => {
                    const nextValue = event.target.value;
                    setNicknameDraft(nextValue);
                    if (!nicknameComposingRef.current) commitNickname(nextValue);
                  }}
                  onCompositionStart={() => { nicknameComposingRef.current = true; }}
                  onCompositionEnd={(event) => {
                    nicknameComposingRef.current = false;
                    commitNickname(event.currentTarget.value);
                  }}
                  disabled={disabled}
                />
              </div>
            </div>
          </div>

          <div className="connect-gate__actions">
            <button
              type="button"
              className="connect-gate__btn connect-gate__btn--primary"
              onClick={onCreate}
              disabled={!canCreate}
            >
              创建新局
            </button>
            <button
              type="button"
              className="connect-gate__btn connect-gate__btn--secondary"
              onClick={onJoin}
              disabled={!canJoin}
            >
              加入牌桌
            </button>
          </div>

          {onWatch && (
            <div className="connect-gate__spectate-action">
              <button
                type="button"
                className="connect-gate__btn connect-gate__btn--spectate"
                onClick={onWatch}
                disabled={!canWatch}
              >
                观战牌桌
              </button>
            </div>
          )}
        </section>

      </div>
    </main>
  );
}
