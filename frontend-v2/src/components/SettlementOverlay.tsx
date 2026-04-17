import { useEffect, useMemo, useState } from "react";
import type {
  ContinueActionView,
  MatchResultPayload,
  RoomSnapshot,
} from "../types/protocol";

interface Props {
  snapshot: RoomSnapshot;
  result: MatchResultPayload | null;
  onContinue: () => void;
  onRestart: () => void;
  onLeave: () => void;
}

function titleOf(result: MatchResultPayload | null): { title: string; sub: string } {
  if (!result) return { title: "本局结束", sub: "静待下一局" };
  if (result.win_type === "draw") {
    return {
      title: "流局",
      sub: result.draw_type === "skill_forced" ? "技能强制结束" : "荒牌流局",
    };
  }
  if (result.win_type === "self_draw") {
    return { title: "自摸", sub: result.display_win_label ?? "自摸胡牌" };
  }
  return { title: result.display_win_label ?? "荣和", sub: "和牌结算" };
}

export function SettlementOverlay({
  snapshot,
  result,
  onContinue,
  onRestart,
  onLeave,
}: Props) {
  const [active, setActive] = useState(false);

  useEffect(() => {
    if (snapshot.phase !== "settlement" && snapshot.phase !== "finished") {
      setActive(false);
      return;
    }
    const t = window.setTimeout(() => setActive(true), 50);
    return () => window.clearTimeout(t);
  }, [snapshot.phase, result?.round_id]);

  const { title, sub } = titleOf(result);
  const cont = snapshot.continue_action;
  const isFinished = snapshot.phase === "finished";

  const deltas = useMemo(() => {
    const delta = result?.score_delta?.total_delta_by_seat ?? {};
    const cumulative = snapshot.match_state?.cumulative_scores ?? {};
    return snapshot.seats.map((s) => {
      const d = delta[String(s.seat_index)] ?? 0;
      const total = cumulative[String(s.seat_index)] ?? 0;
      return {
        seat: s.seat_index,
        name: s.nickname ?? `座位${s.seat_index + 1}`,
        delta: d,
        total,
      };
    });
  }, [snapshot, result]);

  if (snapshot.phase !== "settlement" && snapshot.phase !== "finished") {
    return null;
  }

  return (
    <div className={`ink-layer ${active ? "active" : ""}`}>
      <div className="ink-title">{title}</div>
      <div className="ink-subtitle">{sub}</div>

      {result?.fan_keys?.length ? (
        <div className="ink-fan-list">
          {result.fan_breakdown.map((f) => (
            <span key={f.fan_key} className="ink-fan-tag">
              {fanLabel(f.fan_key)} · {f.fan_value}番
            </span>
          ))}
        </div>
      ) : null}

      <div className="ink-score">
        {deltas.map((d) => (
          <div key={d.seat} className="ink-score-cell">
            <div className="name">{d.name}</div>
            <div
              className={`delta ${d.delta > 0 ? "positive" : d.delta < 0 ? "negative" : ""}`}
            >
              {d.delta > 0 ? `+${d.delta}` : d.delta}
            </div>
            <div className="total">总 {d.total}</div>
          </div>
        ))}
      </div>

      <div className="ink-actions">
        <button type="button" onClick={onLeave}>
          离桌
        </button>
        {isFinished ? (
          <button type="button" className="primary" onClick={onRestart}>
            再开一场
          </button>
        ) : (
          <ContinueButton continueAction={cont} onContinue={onContinue} />
        )}
      </div>
      {cont && cont.auto_advance_deadline_at ? (
        <AutoAdvance deadline={cont.auto_advance_deadline_at} />
      ) : null}
    </div>
  );
}

function ContinueButton({
  continueAction,
  onContinue,
}: {
  continueAction: ContinueActionView | null;
  onContinue: () => void;
}) {
  if (!continueAction) {
    return (
      <button type="button" className="primary" onClick={onContinue}>
        下一局
      </button>
    );
  }
  const me = continueAction.confirmed_seats;
  const required = continueAction.required_seats.length;
  const done = me.length;
  const hasConfirmed = done >= required;
  return (
    <button
      type="button"
      className="primary"
      onClick={onContinue}
      disabled={hasConfirmed}
    >
      {hasConfirmed ? `已确认 ${done}/${required}` : "下一局"}
    </button>
  );
}

function AutoAdvance({ deadline }: { deadline: string }) {
  const [left, setLeft] = useState(() =>
    Math.max(0, Math.round((Date.parse(deadline) - Date.now()) / 1000)),
  );
  useEffect(() => {
    const id = window.setInterval(() => {
      setLeft(Math.max(0, Math.round((Date.parse(deadline) - Date.now()) / 1000)));
    }, 500);
    return () => window.clearInterval(id);
  }, [deadline]);
  return <div className="ink-autohint">{left}s 后自动推进</div>;
}

function fanLabel(k: string) {
  const map: Record<string, string> = {
    test_fan: "测试番",
    big_four_winds: "大四喜",
    all_winds: "字一色",
  };
  return map[k] ?? k;
}
