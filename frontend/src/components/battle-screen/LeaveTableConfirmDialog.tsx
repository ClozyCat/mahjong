import { createPortal } from 'react-dom';

interface LeaveTableConfirmDialogProps {
  isOpen: boolean;
  tableCode?: string;
  onCancel: () => void;
  onConfirm: () => void;
}

export function LeaveTableConfirmDialog({
  isOpen,
  tableCode = '',
  onCancel,
  onConfirm,
}: LeaveTableConfirmDialogProps) {
  if (!isOpen || typeof document === 'undefined') {
    return null;
  }

  return createPortal(
    <div className="leave-table-confirm__backdrop" role="presentation">
      <section
        className="leave-table-confirm__dialog"
        role="dialog"
        aria-modal="true"
        aria-label="确认离席"
      >
        <div className="leave-table-confirm__mark" aria-hidden="true">
          <svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round">
            <path d="M14 6h3a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1h-3" />
            <path d="m10 15 3-3-3-3" />
            <path d="M13 12H6" />
          </svg>
        </div>
        <div className="leave-table-confirm__copy">
          <span className="leave-table-confirm__eyebrow">退出牌桌</span>
          <h2>确认离席？</h2>
          <p>
            {tableCode
              ? `离开牌桌 ${tableCode} 后，需要重新加入才能回到本桌。`
              : '离开后，需要重新加入才能回到本桌。'}
          </p>
        </div>
        <div className="leave-table-confirm__actions">
          <button type="button" className="leave-table-confirm__cancel" onClick={onCancel}>
            取消
          </button>
          <button type="button" className="leave-table-confirm__confirm" onClick={onConfirm}>
            确认离席
          </button>
        </div>
      </section>
    </div>,
    document.body,
  );
}
