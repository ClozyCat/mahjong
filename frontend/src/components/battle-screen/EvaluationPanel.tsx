import { useState } from 'react';
import type { EvaluationSessionResponse } from '../../types/match';

interface EvaluationPanelProps {
  session: EvaluationSessionResponse | null;
  onRefresh?: () => void;
}

export function EvaluationPanel({ session, onRefresh }: EvaluationPanelProps) {
  const [isCollapsed, setIsCollapsed] = useState(false);

  if (!session) {
    return null;
  }
  const completedCount = session.subjects.filter((subject) => subject.completed).length;

  return (
    <div className={isCollapsed ? 'evaluation-panel evaluation-panel--collapsed' : 'evaluation-panel'} aria-label="评测结果">
      <div className="evaluation-panel__header">
        <div className="evaluation-panel__title">
          <strong>评测结果</strong>
          {!isCollapsed && <small>{completedCount}/{session.subjects.length} 完成</small>}
        </div>
        <div className="evaluation-panel__actions">
          <button type="button" onClick={() => setIsCollapsed((current) => !current)}>
            {isCollapsed ? '展开' : '收起'}
          </button>
          {onRefresh && !isCollapsed ? (
            <button type="button" onClick={onRefresh}>刷新</button>
          ) : null}
        </div>
      </div>
      {isCollapsed ? null : (
        <div className="evaluation-panel__rows">
          {session.subjects.map((subject) => (
            <div key={subject.subject_id} className="evaluation-panel__row">
              <div className="evaluation-panel__identity">
                <span>{subject.display_name}</span>
                <small>{subject.kind === 'bot' ? 'AI' : '真人'}</small>
              </div>
              <strong>{subject.final_score ?? '-'}</strong>
              <small>已完成 {formatCount(subject.completed_round_count)} 局</small>
              <small>和牌 {formatCount(subject.win_count)} 次</small>
              <small>放铳 {formatCount(subject.deal_in_count)} 次</small>
              <small>听牌和 {formatReadyHandWinCount(subject)} 次</small>
              <em className={`evaluation-panel__status evaluation-panel__status--${subject.completed ? 'completed' : subject.phase}`}>
                {subject.completed ? '完成' : phaseLabel(subject.phase)}
              </em>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function formatCount(value?: number | null) {
  return value ?? 0;
}

function formatReadyHandWinCount(subject: EvaluationSessionResponse['subjects'][number]) {
  if (subject.kind === 'bot') {
    return 0;
  }
  return formatCount(subject.ready_hand_win_count);
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
