import type { EvaluationSessionResponse } from '../../types/match';

interface EvaluationPanelProps {
  session: EvaluationSessionResponse | null;
  onRefresh?: () => void;
  onOpenTable?: (tableCode: string) => void;
}

export function EvaluationPanel({ session, onRefresh, onOpenTable }: EvaluationPanelProps) {
  if (!session) {
    return null;
  }

  return (
    <div className="evaluation-panel" aria-label="评测结果">
      <div className="evaluation-panel__header">
        <strong>评测结果</strong>
        {onRefresh ? (
          <button type="button" onClick={onRefresh}>刷新</button>
        ) : null}
      </div>
      <div className="evaluation-panel__rows">
        {session.subjects.map((subject) => (
          <div key={subject.subject_id} className="evaluation-panel__row">
            <button type="button" onClick={() => onOpenTable?.(subject.table_code)}>
              {subject.table_code}
            </button>
            <span>{subject.display_name}</span>
            <strong>{subject.final_score ?? '-'}</strong>
            <small>放铳 {subject.deal_in_count ?? '-'}</small>
            <small>和牌 {subject.win_count ?? '-'}</small>
            <em>{subject.completed ? '完成' : phaseLabel(subject.phase)}</em>
          </div>
        ))}
      </div>
    </div>
  );
}

function phaseLabel(phase: string) {
  switch (phase) {
    case 'waiting':
      return '待开始';
    case 'playing':
      return '进行中';
    case 'settlement':
      return '结算中';
    case 'finished':
      return '完成';
    default:
      return phase;
  }
}
