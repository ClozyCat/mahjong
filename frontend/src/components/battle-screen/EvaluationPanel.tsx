import { useState, useEffect, useRef } from 'react';
import type { EvaluationSessionResponse } from '../../types/match';

interface EvaluationPanelProps {
  session: EvaluationSessionResponse | null;
  onRefresh?: () => void;
}

export function EvaluationPanel({ session, onRefresh }: EvaluationPanelProps) {
  const [isCollapsed, setIsCollapsed] = useState(false);
  const [position, setPosition] = useState({ x: 0, y: 0 });
  const [isDragging, setIsDragging] = useState(false);
  const dragStartRef = useRef({ x: 0, y: 0 });
  const panelRef = useRef<HTMLDivElement>(null);
  const positionRef = useRef(position);
  const dragLimitsRef = useRef<{
    parentRect: DOMRect;
    panelRect: DOMRect;
    initialPanelLeft: number;
    initialPanelTop: number;
  } | null>(null);

  // Sync position state to ref to avoid stale closures in resize handler
  useEffect(() => {
    positionRef.current = position;
  }, [position]);

  // Reset drag position back to (0, 0) when session is cleared/finished
  useEffect(() => {
    if (!session) {
      setPosition({ x: 0, y: 0 });
    }
  }, [session]);

  const startDrag = (clientX: number, clientY: number) => {
    if (!panelRef.current) return;
    const parent = panelRef.current.parentElement;
    if (!parent) return;

    const parentRect = parent.getBoundingClientRect();
    const panelRect = panelRef.current.getBoundingClientRect();

    dragLimitsRef.current = {
      parentRect,
      panelRect,
      initialPanelLeft: panelRect.left - position.x,
      initialPanelTop: panelRect.top - position.y,
    };

    setIsDragging(true);
    dragStartRef.current = {
      x: clientX - position.x,
      y: clientY - position.y,
    };
  };

  const handleMouseDown = (e: React.MouseEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    const target = e.target as HTMLElement;
    if (target.closest('button')) return;
    startDrag(e.clientX, e.clientY);
  };

  const handleTouchStart = (e: React.TouchEvent<HTMLDivElement>) => {
    const target = e.target as HTMLElement;
    if (target.closest('button')) return;
    if (e.touches.length === 0) return;
    const touch = e.touches[0];
    startDrag(touch.clientX, touch.clientY);
  };

  useEffect(() => {
    if (!isDragging) return;

    const handleMouseMove = (e: MouseEvent) => {
      const limits = dragLimitsRef.current;
      if (!limits) return;

      const proposedX = e.clientX - dragStartRef.current.x;
      const proposedY = e.clientY - dragStartRef.current.y;

      const left = limits.initialPanelLeft + proposedX;
      const top = limits.initialPanelTop + proposedY;

      const boundedLeft = Math.max(limits.parentRect.left, Math.min(limits.parentRect.right - limits.panelRect.width, left));
      const boundedTop = Math.max(limits.parentRect.top, Math.min(limits.parentRect.bottom - limits.panelRect.height, top));

      setPosition({
        x: boundedLeft - limits.initialPanelLeft,
        y: boundedTop - limits.initialPanelTop,
      });
    };

    const handleMouseUp = () => {
      setIsDragging(false);
    };

    const handleTouchMove = (e: TouchEvent) => {
      if (e.touches.length === 0) return;
      if (e.cancelable) {
        e.preventDefault();
      }

      const touch = e.touches[0];
      const limits = dragLimitsRef.current;
      if (!limits) return;

      const proposedX = touch.clientX - dragStartRef.current.x;
      const proposedY = touch.clientY - dragStartRef.current.y;

      const left = limits.initialPanelLeft + proposedX;
      const top = limits.initialPanelTop + proposedY;

      const boundedLeft = Math.max(limits.parentRect.left, Math.min(limits.parentRect.right - limits.panelRect.width, left));
      const boundedTop = Math.max(limits.parentRect.top, Math.min(limits.parentRect.bottom - limits.panelRect.height, top));

      setPosition({
        x: boundedLeft - limits.initialPanelLeft,
        y: boundedTop - limits.initialPanelTop,
      });
    };

    const handleTouchEnd = () => {
      setIsDragging(false);
    };

    window.addEventListener('mousemove', handleMouseMove);
    window.addEventListener('mouseup', handleMouseUp);
    window.addEventListener('touchmove', handleTouchMove, { passive: false });
    window.addEventListener('touchend', handleTouchEnd);

    return () => {
      window.removeEventListener('mousemove', handleMouseMove);
      window.removeEventListener('mouseup', handleMouseUp);
      window.removeEventListener('touchmove', handleTouchMove);
      window.removeEventListener('touchend', handleTouchEnd);
    };
  }, [isDragging]);

  // Keep panel within container boundaries on window resize/rotation
  useEffect(() => {
    const handleResize = () => {
      if (!panelRef.current) return;
      const parent = panelRef.current.parentElement;
      if (!parent) return;

      const parentRect = parent.getBoundingClientRect();
      const panelRect = panelRef.current.getBoundingClientRect();
      const currentPos = positionRef.current;

      const initialPanelLeft = panelRect.left - currentPos.x;
      const initialPanelTop = panelRect.top - currentPos.y;

      const left = panelRect.left;
      const top = panelRect.top;

      const boundedLeft = Math.max(parentRect.left, Math.min(parentRect.right - panelRect.width, left));
      const boundedTop = Math.max(parentRect.top, Math.min(parentRect.bottom - panelRect.height, top));

      if (boundedLeft !== left || boundedTop !== top) {
        setPosition({
          x: boundedLeft - initialPanelLeft,
          y: boundedTop - initialPanelTop,
        });
      }
    };

    window.addEventListener('resize', handleResize);
    return () => window.removeEventListener('resize', handleResize);
  }, []);

  if (!session) {
    return null;
  }
  const completedCount = session.subjects.filter((subject) => subject.completed).length;

  interface GroupedSubject {
    display_name: string;
    kind: string;
    items: typeof session.subjects;
  }

  const groupedSubjects: GroupedSubject[] = [];
  session.subjects.forEach((subject) => {
    const existing = groupedSubjects.find(
      (g) => g.display_name === subject.display_name && g.kind === subject.kind
    );
    if (existing) {
      existing.items.push(subject);
    } else {
      groupedSubjects.push({
        display_name: subject.display_name,
        kind: subject.kind,
        items: [subject],
      });
    }
  });

  return (
    <div
      ref={panelRef}
      className={isCollapsed ? 'evaluation-panel evaluation-panel--collapsed' : 'evaluation-panel'}
      aria-label="评测结果"
      style={{
        transform: `translate(${position.x}px, ${position.y}px)`,
        transition: isDragging ? 'none' : undefined,
      }}
    >
      <div
        className="evaluation-panel__header"
        onMouseDown={handleMouseDown}
        onTouchStart={handleTouchStart}
        style={{ cursor: isDragging ? 'grabbing' : 'grab' }}
      >
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
        <div className="evaluation-panel__table-container">
          <table className="evaluation-panel__table">
            <thead>
              <tr>
                <th>角色</th>
                <th>类型</th>
                <th>得分</th>
                <th>局数</th>
                <th className="evaluation-panel__col-wins">和牌</th>
                <th className="evaluation-panel__col-deal-ins">放铳</th>
                <th className="evaluation-panel__col-status">状态</th>
              </tr>
            </thead>
            <tbody>
              {groupedSubjects.map((group) =>
                group.items.map((subject, index) => (
                  <tr key={subject.subject_id} className="evaluation-panel__tr">
                    {index === 0 && (
                      <>
                        <td rowSpan={group.items.length} className="evaluation-panel__name-cell">
                          {group.display_name}
                        </td>
                        <td rowSpan={group.items.length} className="evaluation-panel__kind-cell">
                          {group.kind === 'bot' ? 'AI' : '真人'}
                        </td>
                      </>
                    )}
                    <td className="evaluation-panel__score-cell">
                      <strong>{subject.final_score ?? '-'}</strong>
                    </td>
                    <td>{formatCount(subject.completed_round_count)} 局</td>
                    <td className="evaluation-panel__col-wins">{formatCount(subject.win_count)} 次</td>
                    <td className="evaluation-panel__col-deal-ins">{formatCount(subject.deal_in_count)} 次</td>
                    <td className="evaluation-panel__col-status">
                      <em className={`evaluation-panel__status evaluation-panel__status--${subject.completed ? 'completed' : subject.phase}`}>
                        {subject.completed ? '完成' : phaseLabel(subject.phase)}
                      </em>
                    </td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}

function formatCount(value?: number | null) {
  return value ?? 0;
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
