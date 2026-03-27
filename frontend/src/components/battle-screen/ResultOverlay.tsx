import type { BattleActionId, ResultView } from '../../types/match';

interface ResultOverlayProps {
  result: ResultView;
  onAction: (actionId: BattleActionId) => void;
}

export function ResultOverlay({ result, onAction }: ResultOverlayProps) {
  const winTypeLabel = result.winType ? WIN_TYPE_LABELS[result.winType] ?? result.winType : null;

  return (
    <section className="result-overlay" aria-label="Match settlement result">
      <div className="result-overlay__card">
        <span className="result-overlay__eyebrow">结算面板</span>
        <h2>{result.title}</h2>
        <p>{result.summary}</p>
        {result.fanTotal !== null ? (
          <p>
            番数合计 {result.fanTotal}
            {winTypeLabel ? ` · ${winTypeLabel}` : ''}
            {result.winnerSeat ? ` · 胜者 ${result.winnerSeat}` : ''}
            {result.discarderSeat ? ` · 放铳 ${result.discarderSeat}` : ''}
            {result.flowerCount > 0 ? ` · 花牌 ${result.flowerCount}` : ''}
          </p>
        ) : null}
        {result.provisional ? <p className="result-overlay__provisional">当前为临时结算结果</p> : null}

        {result.fanBreakdown.length > 0 ? (
          <div className="result-overlay__list">
            {result.fanBreakdown.map((item) => (
              <div key={item.fanKey} className="result-overlay__row">
                <span>{item.fanKey}</span>
                <strong>{item.fanValue}</strong>
              </div>
            ))}
          </div>
        ) : null}

        <div className="result-overlay__seat-list">
          {result.seats.map((seat) => (
            <div key={`${seat.seat}-${seat.name}`} className="result-overlay__seat-row">
              <span>{seat.name}</span>
              <strong>{seat.score}</strong>
              <span>{seat.delta === null ? '总分' : `${seat.delta > 0 ? '+' : ''}${seat.delta}`}</span>
            </div>
          ))}
        </div>

        {result.continueAction ? (
          <div className="result-overlay__actions">
            <button
              type="button"
              disabled={!result.continueAction.enabled}
              onClick={() => onAction(result.continueAction!.id)}
            >
              {result.continueAction.label}
            </button>
          </div>
        ) : null}
      </div>
    </section>
  );
}

const WIN_TYPE_LABELS: Record<string, string> = {
  discard: '荣和',
  self_draw: '自摸',
  draw: '流局',
};
