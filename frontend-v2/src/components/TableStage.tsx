import { useEffect, useMemo, useState } from "react";
import type {
  EquippedSkillView,
  PendingAction,
  PrivateState,
  PublicSeatView,
  RoomSnapshot,
} from "../types/protocol";
import type { ChatBubble } from "../lib/session";
import { seatPosition, type SeatPosition } from "../lib/tileUtils";
import { SeatTag } from "./SeatTag";
import { OpponentHand } from "./OpponentHand";
import { MeldArea } from "./MeldArea";
import { MyHand } from "./MyHand";
import { FlowerStrip } from "./FlowerStrip";
import { River } from "./River";
import { Compass } from "./Compass";
import { ActionDock } from "./ActionDock";
import type { ActionDockEmit } from "./ActionDock";
import { QuickChat } from "./QuickChat";
import { ChatBubbles } from "./ChatBubbles";
import {
  buildChowCandidates,
  buildClaimKongCandidates,
  buildPungCandidates,
  buildSelfKongCandidates,
} from "../lib/claim";

interface Props {
  snapshot: RoomSnapshot;
  pending: PendingAction | null;
  selectedTileId: string | null;
  onSelectTile: (id: string | null) => void;
  onEmitAction: (emit: ActionDockEmit) => void;
  optimisticDiscardId: string | null;
  promptDeadline: string | null;
  chatBubbles: ChatBubble[];
  onChatSend: (target: number, emoji: string) => void;
  onChatExpire: (id: string) => void;
  onLeave: () => void;
}

const WINDS = ["東", "南", "西", "北"];

export function TableStage({
  snapshot,
  pending,
  selectedTileId,
  onSelectTile,
  onEmitAction,
  optimisticDiscardId,
  promptDeadline,
  chatBubbles,
  onChatSend,
  onChatExpire,
  onLeave,
}: Props) {
  const priv = snapshot.private_state;
  const localSeat = snapshot.local_seat;
  const me = priv?.players.find((p) => p.seat_index === localSeat);
  const restricted = useMemo(() => {
    if (pending?.type === "active_turn") {
      return new Set(pending.restricted_discard_tile_ids);
    }
    return new Set<string>();
  }, [pending]);

  const drawnTileId =
    pending?.type === "active_turn" ? pending.drawn_tile_id : null;

  const selfKongList = useMemo(() => {
    if (!me?.concealed_tiles) return [];
    if (pending?.type !== "active_turn") return [];
    const opts = new Set(pending.options);
    if (!opts.has("kong")) return [];
    return buildSelfKongCandidates(me.concealed_tiles, me.melds);
  }, [pending, me]);

  const claimKongList = useMemo(() => {
    if (!priv?.last_discard || !me?.concealed_tiles) return [];
    if (pending?.type !== "claim_window") return [];
    const opts = new Set(pending.options);
    if (!opts.has("kong")) return [];
    return buildClaimKongCandidates(me.concealed_tiles, priv.last_discard);
  }, [pending, priv, me]);

  const chowList = useMemo(() => {
    if (!priv?.last_discard || !me?.concealed_tiles) return [];
    if (pending?.type !== "claim_window") return [];
    const opts = new Set(pending.options);
    if (!opts.has("chow")) return [];
    return buildChowCandidates(me.concealed_tiles, priv.last_discard);
  }, [pending, priv, me]);

  const pungList = useMemo(() => {
    if (!priv?.last_discard || !me?.concealed_tiles) return [];
    if (pending?.type !== "claim_window") return [];
    const opts = new Set(pending.options);
    if (!opts.has("pung")) return [];
    return buildPungCandidates(me.concealed_tiles, priv.last_discard);
  }, [pending, priv, me]);

  // 本家多种 kong 候选时弹出本地提示(单候选由 ActionDock 直接发送)
  const [kongPromptVisible, setKongPromptVisible] = useState(false);
  useEffect(() => {
    if (pending?.type === "active_turn" && selfKongList.length > 1) {
      setKongPromptVisible(true);
    } else {
      setKongPromptVisible(false);
    }
  }, [pending, selfKongList.length]);
  const shouldShowKongPrompt = kongPromptVisible && selfKongList.length > 1;

  const equipped = priv?.equipped_skills ?? [];

  const seatsByPos: Record<SeatPosition, PublicSeatView | undefined> = {
    bottom: undefined,
    left: undefined,
    top: undefined,
    right: undefined,
  };
  for (const s of snapshot.seats) {
    seatsByPos[seatPosition(s.seat_index, localSeat)] = s;
  }

  const cumulative = snapshot.match_state?.cumulative_scores ?? {};
  const windForSeat = (seatIdx: number) => {
    const rel = (seatIdx - (snapshot.match_state?.dealer_seat ?? 0) + 4) % 4;
    return WINDS[rel];
  };

  const renderOpponentBlock = (pos: SeatPosition) => {
    const seat = seatsByPos[pos];
    if (!seat || seat.seat_index === localSeat) return null;
    const player = priv?.players.find((p) => p.seat_index === seat.seat_index);
    const concealedCount = player?.concealed_count ?? 13;
    const isCurrent = priv?.current_actor === seat.seat_index;
    const orientation = pos === "top" ? "horizontal" : "vertical";
    const mirrored = pos === "right";

    const seatHeader = (
      <SeatTag
        seat={seat}
        isLocal={false}
        isCurrent={isCurrent}
        wind={windForSeat(seat.seat_index)}
        cumulativeScore={cumulative[String(seat.seat_index)] ?? 0}
      />
    );
    const block = (
      <div className={`player-area ${pos}`}>
        {pos === "left" || pos === "right" ? (
          <>
            <div className="side-identity">
              {seatHeader}
              <FlowerStrip flowers={player?.flowers ?? []} />
            </div>
            <div className="side-tiles">
              <OpponentHand
                count={concealedCount}
                orientation="vertical"
                size="sm"
              />
              {player?.melds.length ? (
                <div className="side-meld-wrap">
                  <MeldArea
                    melds={player.melds}
                    orientation="vertical"
                    size="sm"
                    mirrored={mirrored}
                  />
                </div>
              ) : null}
            </div>
          </>
        ) : (
          <>
            <div className="top-identity">
              {seatHeader}
              <FlowerStrip flowers={player?.flowers ?? []} />
            </div>
            <OpponentHand count={concealedCount} orientation={orientation} size="sm" />
            {player?.melds.length ? (
              <div className="top-meld-wrap">
                <MeldArea melds={player.melds} orientation="horizontal" size="sm" />
              </div>
            ) : null}
          </>
        )}
      </div>
    );
    return block;
  };

  const isMyTurn = pending?.type === "active_turn" && pending.seat_index === localSeat;

  return (
    <>
      <div className="table-stage">
        <div className="table-top-bar">
          <div className="round-banner glass">
            <span className="chip">
              {
                {
                  east: "東",
                  south: "南",
                  west: "西",
                  north: "北",
                }[snapshot.match_state?.prevailing_wind ?? "east"]
              }
              風局
            </span>
            <div>
              <div>第 {snapshot.match_state?.hand_number ?? 1} 场</div>
              <div className="sub">{codeLabel(snapshot.table_code)}</div>
            </div>
          </div>
          <div className="top-menu-row">
            <div className="round-banner glass" style={{ fontSize: 12, letterSpacing: 2 }}>
              {snapshot.mode === "skill" ? "技能局" : snapshot.mode === "test" ? "测试局" : "常规局"}
            </div>
            <button type="button" className="btn-ghost glass" onClick={onLeave}>
              离桌
            </button>
          </div>
        </div>

        {/* 四向玩家区 */}
        {renderOpponentBlock("top")}
        {renderOpponentBlock("left")}
        {renderOpponentBlock("right")}

        {/* 中央:弃牌池+罗盘 */}
        <div className="center-board">
          {renderRiver(priv, localSeat, "top")}
          {renderRiver(priv, localSeat, "left")}
          <Compass
            wind={compassWind(snapshot)}
            handNumber={snapshot.match_state?.hand_number ?? 1}
            wallRemaining={priv?.wall_tiles_remaining ?? 0}
            deadlineAt={promptDeadline}
          />
          {renderRiver(priv, localSeat, "right")}
          {renderRiver(priv, localSeat, "bottom")}
        </div>

        {/* 本家 */}
        <div className="player-area bottom">
          <div className="bottom-identity">
            <SeatTag
              seat={snapshot.seats.find((s) => s.seat_index === localSeat)!}
              isLocal={true}
              isCurrent={priv?.current_actor === localSeat}
              wind={windForSeat(localSeat)}
              cumulativeScore={cumulative[String(localSeat)] ?? 0}
            />
            <FlowerStrip flowers={me?.flowers ?? []} />
          </div>
          {me?.melds.length ? (
            <div className="bottom-meld-wrap">
              <MeldArea melds={me.melds} orientation="horizontal" size="md" />
            </div>
          ) : null}
          {me?.concealed_tiles ? (
            <MyHand
              tiles={me.concealed_tiles}
              drawnTileId={drawnTileId}
              selectedId={selectedTileId}
              restrictedIds={restricted}
              optimisticHiddenId={optimisticDiscardId}
              onSelect={(id) =>
                onSelectTile(id === selectedTileId ? null : id)
              }
              onDoubleDiscard={(id) => {
                if (
                  pending?.type === "active_turn" &&
                  pending.options.includes("discard")
                ) {
                  onEmitAction({ action_type: "discard", tile_ids: [id] });
                  onSelectTile(null);
                }
              }}
            />
          ) : null}
        </div>

        {isMyTurn && !kongPromptVisible ? (
          <ActionDock
            pending={pending}
            onEmit={onEmitAction}
            selfKongCandidates={selfKongList}
            chowCandidates={chowList}
            pungCandidates={pungList}
            claimKongCandidates={claimKongList}
            equippedSkills={equipped as EquippedSkillView[]}
            selectedTileId={selectedTileId}
            onClearSelection={() => onSelectTile(null)}
          />
        ) : pending && pending.type !== "active_turn" && pending.type !== "skill_draft" ? (
          <ActionDock
            pending={pending}
            onEmit={onEmitAction}
            selfKongCandidates={selfKongList}
            chowCandidates={chowList}
            pungCandidates={pungList}
            claimKongCandidates={claimKongList}
            equippedSkills={equipped as EquippedSkillView[]}
            selectedTileId={selectedTileId}
            onClearSelection={() => onSelectTile(null)}
          />
        ) : null}

        {shouldShowKongPrompt ? (
          <div className="kong-prompt">
            <div className="kong-title">是否杠牌?</div>
            <div className="kong-options">
              {selfKongList.map((c, i) => (
                <button
                  key={i}
                  type="button"
                  className="kong-option"
                  onClick={() => {
                    onEmitAction({ action_type: "kong", tile_ids: c.tileIds });
                    setKongPromptVisible(false);
                  }}
                >
                  {c.previewKeys.map((k) => describeShort(k)).join("")}
                </button>
              ))}
              <button
                type="button"
                className="kong-option"
                onClick={() => setKongPromptVisible(false)}
              >
                暂不
              </button>
            </div>
          </div>
        ) : null}

        <ChatBubbles
          bubbles={chatBubbles}
          localSeat={localSeat}
          onExpire={onChatExpire}
        />
        <QuickChat
          localSeat={localSeat}
          seats={snapshot.seats}
          onSend={onChatSend}
        />
      </div>
    </>
  );
}

function renderRiver(
  priv: PrivateState | null,
  localSeat: number,
  pos: SeatPosition,
) {
  if (!priv) return <div />;
  const seatForPos = (0 + localSeat + (pos === "bottom" ? 0 : pos === "right" ? 1 : pos === "top" ? 2 : 3)) % 4;
  const player = priv.players.find((p) => p.seat_index === seatForPos);
  const discards = player?.discards ?? [];
  const latest =
    priv.last_discard && discards.length > 0
      ? discards[discards.length - 1]
      : null;
  return (
    <River
      discards={discards}
      latestKey={latest && latest === priv.last_discard ? latest : null}
      position={pos}
    />
  );
}

function compassWind(snap: RoomSnapshot): string {
  const w = snap.match_state?.prevailing_wind ?? "east";
  return { east: "東", south: "南", west: "西", north: "北" }[w];
}

function codeLabel(code: string) {
  return code.split("").join(" ");
}

function describeShort(k: string) {
  // 简短牌名
  const s = k.toLowerCase();
  if (/^w[1-9]$/.test(s)) return "万" + s[1];
  if (/^t[1-9]$/.test(s)) return "条" + s[1];
  if (/^b[1-9]$/.test(s)) return "筒" + s[1];
  const map: Record<string, string> = {
    east: "東",
    south: "南",
    west: "西",
    north: "北",
    red: "中",
    green: "發",
    white: "白",
  };
  return map[s] ?? s;
}
