import { useEffect, useState } from "react";

interface Props {
  keyMarker: string;
  title: string;
  subtitle?: string;
  onDone?: () => void;
}

export function EntranceTransition({ keyMarker, title, subtitle, onDone }: Props) {
  const [opened, setOpened] = useState(false);
  const [fading, setFading] = useState(false);
  const [mounted, setMounted] = useState(true);

  useEffect(() => {
    setMounted(true);
    setOpened(false);
    setFading(false);
    const t1 = window.setTimeout(() => setOpened(true), 50);
    const t2 = window.setTimeout(() => setFading(true), 2200);
    const t3 = window.setTimeout(() => {
      setMounted(false);
      onDone?.();
    }, 2900);
    return () => {
      window.clearTimeout(t1);
      window.clearTimeout(t2);
      window.clearTimeout(t3);
    };
  }, [keyMarker, onDone]);

  if (!mounted) return null;
  return (
    <div
      className={`scroll-transition ${opened ? "opened" : ""} ${fading ? "fading" : ""}`}
    >
      <div className="scroll-content">
        <div className="title">{title}</div>
        {subtitle ? <div className="sub">{subtitle}</div> : null}
      </div>
    </div>
  );
}
