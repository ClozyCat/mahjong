import { useState } from "react";
import { ApiError, createTable } from "../lib/api";
import { getWsBaseUrl } from "../lib/env";
import type { RoomMode } from "../types/protocol";

interface Props {
  onConnect: (args: {
    tableCode: string;
    nickname: string;
    wsBaseUrl: string;
  }) => void;
  initialError?: string | null;
}

const MODES: { value: RoomMode; label: string; desc: string }[] = [
  { value: "normal", label: "常规", desc: "标准规则" },
  { value: "skill", label: "技能", desc: "两局一轮的技能签启" },
  { value: "test", label: "测试", desc: "自动补满 bot 立即开局" },
];

export function Lobby({ onConnect, initialError }: Props) {
  const [mode, setMode] = useState<RoomMode>("normal");
  const [nickname, setNickname] = useState("");
  const [tableCode, setTableCode] = useState("");
  const [eightFan, setEightFan] = useState(true);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(initialError ?? null);

  const submit = async (intent: "create" | "join") => {
    const name = nickname.trim() || "旅人";
    const rawCode = tableCode.trim().toUpperCase();
    if (intent === "join" && !rawCode) {
      setError("请填写牌桌号以加入");
      return;
    }
    setLoading(true);
    setError(null);
    try {
      let finalCode = rawCode;
      if (intent === "create") {
        const res = await createTable({
          table_code: rawCode || undefined,
          mode,
          enforce_minimum_eight_fan: eightFan,
        });
        finalCode = res.table_code;
      }
      onConnect({
        tableCode: finalCode,
        nickname: name,
        wsBaseUrl: getWsBaseUrl(),
      });
    } catch (err) {
      if (err instanceof ApiError) {
        if (err.detail === "table_code_exists") {
          const confirmJoin = window.confirm(
            `牌桌 ${rawCode} 已存在,是否直接加入?`,
          );
          if (confirmJoin) {
            onConnect({
              tableCode: rawCode,
              nickname: name,
              wsBaseUrl: getWsBaseUrl(),
            });
            setLoading(false);
            return;
          }
          setError(null);
        } else {
          setError(translateHttpError(err.detail));
        }
      } else {
        setError((err as Error).message || "创建失败");
      }
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="lobby">
      <div className="lobby-panel glass">
        <div>
          <div className="lobby-title">雅奢麻将</div>
          <div className="lobby-subtitle">绢丝水墨 · 极简对弈</div>
        </div>

        {error ? <div className="lobby-error">{error}</div> : null}

        <div className="lobby-field">
          <label>昵称</label>
          <input
            className="lobby-input"
            value={nickname}
            onChange={(e) => setNickname(e.target.value)}
            maxLength={12}
            placeholder="落笔为名"
          />
        </div>

        <div className="lobby-field">
          <label>牌桌号(可选:留空则自动生成)</label>
          <input
            className="lobby-input"
            value={tableCode}
            onChange={(e) =>
              setTableCode(
                e.target.value
                  .toUpperCase()
                  .replace(/[^A-Z0-9]/g, "")
                  .slice(0, 12),
              )
            }
            placeholder="AB12CD"
          />
        </div>

        <div className="lobby-field">
          <label>模式</label>
          <div className="lobby-modes">
            {MODES.map((m) => (
              <button
                key={m.value}
                type="button"
                className={`mode-chip ${mode === m.value ? "active" : ""}`}
                onClick={() => setMode(m.value)}
                title={m.desc}
              >
                {m.label}
              </button>
            ))}
          </div>
        </div>

        <div className="lobby-eightfan">
          <span>八番起胡</span>
          <div
            className={`toggle ${eightFan ? "on" : ""}`}
            onClick={() => setEightFan((x) => !x)}
            role="switch"
            aria-checked={eightFan}
          />
        </div>

        <div className="lobby-actions">
          <button
            type="button"
            className="btn-secondary"
            disabled={loading}
            onClick={() => submit("join")}
          >
            加入
          </button>
          <button
            type="button"
            className="btn-primary"
            disabled={loading}
            onClick={() => submit("create")}
          >
            {loading ? "请稍候" : "新建"}
          </button>
        </div>

        <div className="lobby-tip">创建后立即进入对弈,四席齐备方可启局</div>
      </div>
    </div>
  );
}

function translateHttpError(detail: string): string {
  const map: Record<string, string> = {
    invalid_table_code: "牌桌号仅限大写字母与数字",
    unsupported_mode: "模式不受支持",
    table_code_exists: "牌桌号已存在",
  };
  return map[detail] ?? detail;
}
