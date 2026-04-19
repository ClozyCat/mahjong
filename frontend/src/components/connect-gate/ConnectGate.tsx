import { useEffect, useRef, useState } from 'react';

import type { TableMode } from '../../types/match';

export interface ConnectGateValue {
  tableCode: string;
  nickname: string;
  tableMode: TableMode;
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

const TABLE_MODE_COPY: Record<TableMode, { label: string; description: string }> = {
  normal: {
    label: '普通模式',
    description: '手动准备',
  },
  test: {
    label: '测试模式',
    description: '自动补位',
  },
};

const TABLE_MODE_ORDER: TableMode[] = ['normal', 'test'];

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

  function renderTableModeToggle() {
    const currentMode = TABLE_MODE_COPY[value.tableMode];
    const currentModeIndex = TABLE_MODE_ORDER.indexOf(value.tableMode);
    const nextMode = TABLE_MODE_ORDER[(currentModeIndex + 1) % TABLE_MODE_ORDER.length] ?? 'normal';
    const nextModeLabel = TABLE_MODE_COPY[nextMode].label;

    return (
      <button
        type="button"
        className="connect-gate__toggle"
        onClick={() => onChange({ tableMode: nextMode })}
        disabled={disabled}
        aria-pressed={value.tableMode !== 'normal'}
        aria-label={`牌桌模式：${currentMode.label}`}
        title={`点击切换为${nextModeLabel}`}
      >
        <span>模式</span>
        <strong>{currentMode.label}</strong>
      </button>
    );
  }

  return (
    <main className="connect-gate" aria-label="Lobby">
      <div className="connect-gate__backdrop" aria-hidden="true" />
      
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

          <div className="connect-gate__settings">
            {renderTableModeToggle()}
            <button
              type="button"
              className="connect-gate__toggle"
              onClick={() => onChange({ enforceMinimumEightFan: !value.enforceMinimumEightFan })}
              disabled={disabled}
              aria-pressed={value.enforceMinimumEightFan}
            >
              <span>限制</span>
              <strong>{value.enforceMinimumEightFan ? '八番起胡' : '自由模式'}</strong>
            </button>
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
        </section>

        <footer className="connect-gate__footer">
          <p className="connect-gate__quote">{footnoteQuote}</p>
        </footer>
      </div>
    </main>
  );
}
