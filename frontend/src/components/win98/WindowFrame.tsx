import type { ReactNode } from 'react';

interface WindowFrameProps {
  title: string;
  status?: string;
  children: ReactNode;
  className?: string;
}

export function WindowFrame({ title, status, children, className }: WindowFrameProps) {
  return (
    <section className={['win98-window', className].filter(Boolean).join(' ')}>
      <header className="win98-window__titlebar">
        <strong>{title}</strong>
        <div className="win98-window__titlebar-buttons" aria-hidden="true">
          <span />
          <span />
          <span />
        </div>
      </header>
      <div className="win98-window__body">{children}</div>
      {status ? <footer className="win98-window__status">{status}</footer> : null}
    </section>
  );
}
