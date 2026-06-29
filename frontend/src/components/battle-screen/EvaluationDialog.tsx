import type { InviteDialogUser } from './PlayerInviteDialog';

interface EvaluationDialogProps {
  isOpen: boolean;
  users: InviteDialogUser[];
  selectedUserIds: number[];
  currentUserId?: number | null;
  isSubmitting?: boolean;
  onToggleSubject: (userId: number) => void;
  onStart: () => void;
  onClose: () => void;
}

export function EvaluationDialog({
  isOpen,
  users,
  selectedUserIds,
  currentUserId = null,
  isSubmitting = false,
  onToggleSubject,
  onStart,
  onClose,
}: EvaluationDialogProps) {
  if (!isOpen) {
    return null;
  }

  const selectedSet = new Set(selectedUserIds);
  const candidateUsers = users.filter(
    ({ user, status }) => user.user_id !== currentUserId && status === 'online',
  );
  const canSelectMore = selectedUserIds.length < 3;

  return (
    <div className="evaluation-dialog" role="dialog" aria-modal="true" aria-label="创建评测">
      <div className="evaluation-dialog__header">
        <strong>评测对比</strong>
        <button type="button" aria-label="关闭评测窗口" onClick={onClose}>×</button>
      </div>
      <div className="evaluation-dialog__subjects" role="group" aria-label="选择受测者">
        {candidateUsers.map(({ user, status }) => {
          const checked = selectedSet.has(user.user_id);
          return (
            <label key={user.user_id} className="evaluation-dialog__subject">
              <input
                type="checkbox"
                checked={checked}
                disabled={!checked && !canSelectMore}
                onChange={() => onToggleSubject(user.user_id)}
              />
              <span>{user.display_label || user.display_name}</span>
              <small>{statusText(status)}</small>
            </label>
          );
        })}
      </div>
      <div className="evaluation-dialog__actions">
        <button type="button" onClick={onClose}>取消</button>
        <button type="button" disabled={isSubmitting} onClick={onStart}>
          {isSubmitting ? '创建中' : '开始评测'}
        </button>
      </div>
    </div>
  );
}

function statusText(status: string) {
  if (status === 'online') {
    return '在线';
  }
  if (status === 'playing') {
    return '对局中';
  }
  return '离线';
}
