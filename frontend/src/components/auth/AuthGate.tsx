import { useState } from 'react';

type AuthMode = 'login' | 'register';

interface LoginValue {
  identifier: string;
  password: string;
}

interface RegisterValue {
  inviteCode: string;
  displayName: string;
  password: string;
}

interface AuthGateProps {
  status: 'idle' | 'loading' | 'error';
  message?: string | null;
  onLogin: (value: LoginValue) => void;
  onRegister: (value: RegisterValue) => void;
}

export function AuthGate({ status, message, onLogin, onRegister }: AuthGateProps) {
  const [mode, setMode] = useState<AuthMode>('login');
  const [identifier, setIdentifier] = useState('');
  const [loginPassword, setLoginPassword] = useState('');
  const [inviteCode, setInviteCode] = useState('');
  const [displayName, setDisplayName] = useState('');
  const [registerPassword, setRegisterPassword] = useState('');
  const disabled = status === 'loading';
  const canSubmitLogin = identifier.trim().length > 0 && loginPassword.trim().length > 0;
  const canSubmitRegister =
    inviteCode.trim().length > 0 && displayName.trim().length > 0 && registerPassword.trim().length > 0;
  const statusText = message ?? (disabled ? '正在处理请求...' : '登录后即可进入牌桌。');

  return (
    <main className="auth-gate" aria-label="Auth gate">
      <div className="auth-gate__panel">
        <header className="auth-gate__header">
          <p className="auth-gate__eyebrow">Account</p>
          <h1>国标麻将听多响炮版</h1>
          <p className="auth-gate__status" role="status">
            {statusText}
          </p>
        </header>

        <div className="auth-gate__tabs" role="tablist" aria-label="认证模式">
          <button
            type="button"
            role="tab"
            aria-selected={mode === 'login'}
            className={mode === 'login' ? 'is-active' : undefined}
            onClick={() => setMode('login')}
          >
            登录
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={mode === 'register'}
            className={mode === 'register' ? 'is-active' : undefined}
            onClick={() => setMode('register')}
          >
            邀请码注册
          </button>
        </div>

        {mode === 'login' ? (
          <form
            className="auth-gate__form"
            onSubmit={(event) => {
              event.preventDefault();
              onLogin({
                identifier: identifier.trim(),
                password: loginPassword.trim(),
              });
            }}
          >
            <label className="auth-gate__field">
              <span>账号昵称</span>
              <input
                value={identifier}
                onChange={(event) => setIdentifier(event.target.value)}
                disabled={disabled}
                autoComplete="username"
                required
              />
            </label>
            <label className="auth-gate__field">
              <span>密码</span>
              <input
                type="password"
                value={loginPassword}
                onChange={(event) => setLoginPassword(event.target.value)}
                disabled={disabled}
                autoComplete="current-password"
                required
              />
            </label>
            <button type="submit" className="auth-gate__primary" disabled={disabled || !canSubmitLogin}>
              登录
            </button>
          </form>
        ) : (
          <form
            className="auth-gate__form"
            onSubmit={(event) => {
              event.preventDefault();
              onRegister({
                inviteCode: inviteCode.trim(),
                displayName: displayName.trim(),
                password: registerPassword.trim(),
              });
            }}
          >
            <label className="auth-gate__field">
              <span>邀请码</span>
              <input
                value={inviteCode}
                onChange={(event) => setInviteCode(event.target.value)}
                disabled={disabled}
                autoComplete="one-time-code"
                required
              />
            </label>
            <label className="auth-gate__field">
              <span>昵称</span>
              <input
                value={displayName}
                onChange={(event) => setDisplayName(event.target.value)}
                disabled={disabled}
                autoComplete="nickname"
                required
              />
            </label>
            <label className="auth-gate__field">
              <span>密码</span>
              <input
                type="password"
                value={registerPassword}
                onChange={(event) => setRegisterPassword(event.target.value)}
                disabled={disabled}
                autoComplete="new-password"
                required
              />
            </label>
            <button type="submit" className="auth-gate__primary" disabled={disabled || !canSubmitRegister}>
              注册并登录
            </button>
          </form>
        )}
      </div>
    </main>
  );
}
