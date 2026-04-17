import { useEffect } from "react";
import type { ToastItem } from "../lib/session";

export function ToastLayer({
  toasts,
  onDismiss,
}: {
  toasts: ToastItem[];
  onDismiss: (id: string) => void;
}) {
  useEffect(() => {
    const timers = toasts.map((t) =>
      window.setTimeout(() => onDismiss(t.id), 2800),
    );
    return () => timers.forEach((t) => window.clearTimeout(t));
  }, [toasts, onDismiss]);

  return (
    <div className="toast-layer">
      {toasts.map((t) => (
        <div key={t.id} className={`toast ${t.tone}`}>
          {t.message}
        </div>
      ))}
    </div>
  );
}
