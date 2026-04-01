import { useEffect, useState } from 'react';

import type { BattleActionId, ResultView, ResultSeatView, Seat } from '../../types/match';

interface ResultOverlayProps {
  result: ResultView;
  onAction: (actionId: BattleActionId) => void;
}

export function ResultOverlay({ result, onAction }: ResultOverlayProps) {
  const [isExpanded, setIsExpanded] = useState(false);
  const [isCollapsed, setIsCollapsed] = useState(false);
  const winTypeLabel = result.winTypeLabel ?? (result.winType ? WIN_TYPE_LABELS[result.winType] ?? result.winType : null);
  const visibleFanBreakdown = isExpanded ? result.fanBreakdown : result.fanBreakdown.slice(0, MAX_VISIBLE_FAN_ITEMS);
  const hiddenFanCount = Math.max(0, result.fanBreakdown.length - MAX_VISIBLE_FAN_ITEMS);
  const fanMeta = [
    winTypeLabel,
    result.winnerSeat ? `胜者 ${formatResultActor(result.winnerSeat, result.seats)}` : null,
    result.discarderSeat ? `放铳 ${formatResultActor(result.discarderSeat, result.seats)}` : null,
    result.flowerCount > 0 ? `花牌 ${result.flowerCount}` : null,
  ]
    .filter((item): item is string => Boolean(item))
    .join(' · ');

  useEffect(() => {
    setIsExpanded(false);
    setIsCollapsed(false);
  }, [result.fanBreakdown, result.title, result.summary]);

  if (isCollapsed) {
    return (
      <section className="result-overlay result-overlay--collapsed" aria-label="Match settlement result">
        <button
          type="button"
          className="result-overlay__restore"
          onClick={() => setIsCollapsed(false)}
          aria-expanded="false"
        >
          展开结算面板
        </button>
      </section>
    );
  }

  return (
    <section className="result-overlay" aria-label="Match settlement result">
      <div className="result-overlay__card">
        <div className="result-overlay__header">
          <div className="result-overlay__heading">
            <span className="result-overlay__eyebrow">结算面板</span>
            <h2>{result.title}</h2>
          </div>
          <button
            type="button"
            className="result-overlay__collapse"
            onClick={() => setIsCollapsed(true)}
            aria-expanded="true"
          >
            收起结算面板
          </button>
        </div>
        <p className="result-overlay__summary">{result.summary}</p>

        {result.fanTotal !== null || result.fanBreakdown.length > 0 ? (
          <div className="result-overlay__fan-panel">
            <div className="result-overlay__section-head">
              <span className="result-overlay__section-label">番型明细</span>
              {result.fanTotal !== null ? <strong className="result-overlay__fan-total">{result.fanTotal} 番</strong> : null}
            </div>
            {fanMeta ? <p className="result-overlay__fan-meta">{fanMeta}</p> : null}

            {result.fanBreakdown.length > 0 ? (
              <div className="result-overlay__list">
                {visibleFanBreakdown.map((item) => (
                  <div key={item.fanKey} className="result-overlay__row">
                    <span>{getFanLabel(item.fanKey)}</span>
                    <strong>{item.fanValue}</strong>
                  </div>
                ))}
                {hiddenFanCount > 0 ? (
                  <button
                    type="button"
                    className="result-overlay__toggle"
                    onClick={() => setIsExpanded((current) => !current)}
                  >
                    {isExpanded ? '收起番种' : `展开剩余 ${hiddenFanCount} 项番种`}
                  </button>
                ) : null}
              </div>
            ) : null}
          </div>
        ) : null}

        <div className="result-overlay__score-panel">
          <div className="result-overlay__section-head">
            <span className="result-overlay__section-label">玩家分数</span>
            <span className="result-overlay__score-hint">本局结算后总分</span>
          </div>
          <div className="result-overlay__seat-list">
            {result.seats.map((seat) => {
              const deltaClassName =
                seat.delta === null
                  ? 'result-overlay__seat-delta result-overlay__seat-delta--neutral'
                  : seat.delta > 0
                    ? 'result-overlay__seat-delta result-overlay__seat-delta--positive'
                    : seat.delta < 0
                      ? 'result-overlay__seat-delta result-overlay__seat-delta--negative'
                      : 'result-overlay__seat-delta result-overlay__seat-delta--neutral';

              const rowClassName =
                seat.delta !== null && seat.delta > 0
                  ? 'result-overlay__seat-row result-overlay__seat-row--positive'
                  : seat.delta !== null && seat.delta < 0
                    ? 'result-overlay__seat-row result-overlay__seat-row--negative'
                    : 'result-overlay__seat-row result-overlay__seat-row--neutral';

              return (
                <div key={`${seat.seat}-${seat.name}`} className={rowClassName}>
                  <div className="result-overlay__seat-main">
                    <span className="result-overlay__seat-name">{seat.name}</span>
                    <span className="result-overlay__seat-tag">{getRelativeSeatLabel(seat.seat)}</span>
                  </div>
                  <strong className="result-overlay__seat-score">{seat.score}</strong>
                  <span className={deltaClassName}>
                    {seat.delta === null ? '总分' : `${seat.delta > 0 ? '+' : ''}${seat.delta}`}
                  </span>
                </div>
              );
            })}
          </div>
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

const MAX_VISIBLE_FAN_ITEMS = 3;
const RELATIVE_SEAT_LABELS: Record<Seat, string> = {
  bottom: '本家',
  left: '左家',
  top: '对家',
  right: '右家',
};

const FAN_LABELS: Record<string, string> = {
  ping_hu: '平胡',
  self_draw: '自摸',
  self_drawn: '自摸',
  zi_mo: '自摸',
  full_flush: '清一色',
  half_flush: '混一色',
  all_pungs: '对对胡',
  seven_pairs: '七对',
  seven_shifted_pairs: '连七对',
  pure_straight: '一条龙',
  mixed_triple_chow: '三色同顺',
  pure_triple_chow: '一色三同顺',
  triple_pung: '三色同刻',
  pure_double_chow: '一般高',
  mixed_double_chow: '喜相逢',
  mixed_shifted_chows: '三色三步高',
  pure_shifted_chows: '一色三步高',
  four_pure_shifted_chows: '一色四步高',
  short_straight: '连六',
  mixed_straight: '花龙',
  two_terminal_chows: '老少副',
  three_suited_terminal_chows: '三色双龙会',
  pure_terminal_chows: '一色双龙会',
  quadruple_chow: '一色四同顺',
  mixed_shifted_pungs: '三色三节高',
  pure_shifted_pungs: '一色三节高',
  four_pure_shifted_pungs: '一色四节高',
  all_simples: '断幺',
  outside_hand: '全带幺',
  no_honours: '无字',
  one_voided_suit: '缺一门',
  all_terminals: '清幺九',
  mixed_terminals: '混幺九',
  all_terminals_and_honours: '混幺九',
  all_honors: '字一色',
  all_honours: '字一色',
  all_green: '绿一色',
  all_even_pungs: '全双刻',
  big_three_dragons: '大三元',
  little_three_dragons: '小三元',
  big_four_winds: '大四喜',
  little_four_winds: '小四喜',
  big_three_winds: '三风刻',
  two_dragon_pungs: '双箭刻',
  seat_wind: '门风刻',
  prevalent_wind: '圈风刻',
  dragon_pung: '箭刻',
  pung_of_terminals_or_honours: '幺九刻',
  double_pung: '双同刻',
  two_concealed_pungs: '双暗刻',
  three_concealed_pungs: '三暗刻',
  four_concealed_pungs: '四暗刻',
  concealed_hand: '门前清',
  fully_concealed_hand: '不求人',
  men_qian_qing: '门前清',
  melded_hand: '全求人',
  concealed_kong: '暗杠',
  two_concealed_kongs: '双暗杠',
  melded_kong: '明杠',
  two_melded_kongs: '双明杠',
  three_kongs: '三杠',
  four_kongs: '四杠',
  robbing_the_kong: '抢杠和',
  last_tile_draw: '妙手回春',
  last_tile_claim: '海底捞月',
  last_tile: '和绝张',
  out_with_replacement_tile: '杠上开花',
  chicken_hand: '鸡胡',
  all_chows: '平和',
  edge_wait: '边张',
  closed_wait: '嵌张',
  single_wait: '单钓将',
  tile_hog: '四归一',
  flower_tiles: '花牌',
  knitted_straight: '组合龙',
  lesser_honours_and_knitted_tiles: '全不靠',
  greater_honours_and_knitted_tiles: '七星不靠',
  thirteen_orphans: '十三幺',
  all_types: '五门齐',
  all_fives: '全带五',
  upper_four: '大于五',
  upper_tiles: '全大',
  lower_four: '小于五',
  lower_tiles: '全小',
  middle_tiles: '全中',
  reversible_tiles: '推不倒',
  heavenly_hand: '天胡',
  earthly_hand: '地胡',
  human_hand: '人胡',
  nine_gates: '九莲宝灯',
};

function getFanLabel(fanKey: string) {
  if (FAN_LABELS[fanKey]) {
    return FAN_LABELS[fanKey];
  }

  return fanKey;
}

function getRelativeSeatLabel(seat: Seat) {
  return RELATIVE_SEAT_LABELS[seat];
}

function formatResultActor(seat: Seat, seats: ResultSeatView[]) {
  const relativeSeatLabel = getRelativeSeatLabel(seat);
  const seatView = seats.find((item) => item.seat === seat);

  if (!seatView?.name) {
    return relativeSeatLabel;
  }

  return `${seatView.name}（${relativeSeatLabel}）`;
}
