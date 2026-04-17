import { useMemo, useState } from "react";
import type { ClaimCandidate } from "../lib/claim";
import type { EquippedSkillView, PendingAction } from "../types/protocol";
import { describeTile } from "../lib/tileUtils";

export interface ActionDockEmit {
  action_type: string;
  tile_ids: string[];
}

interface Props {
  pending: PendingAction | null;
  onEmit: (payload: ActionDockEmit) => void;
  selfKongCandidates: ClaimCandidate[];
  chowCandidates: ClaimCandidate[];
  pungCandidates: ClaimCandidate[];
  claimKongCandidates: ClaimCandidate[];
  equippedSkills: EquippedSkillView[];
  onSelectTarget?: (skillId: string) => void;
  selectedTileId: string | null;
  onClearSelection: () => void;
}

function tileLabel(k: string): string {
  const d = describeTile(k);
  if (d.suit === "wan") return `${d.label}萬`;
  if (d.suit === "tiao") return `${d.label}條`;
  if (d.suit === "tong") return `${d.label}筒`;
  return d.label;
}

export function ActionDock({
  pending,
  onEmit,
  selfKongCandidates,
  chowCandidates,
  pungCandidates,
  claimKongCandidates,
  equippedSkills,
  onSelectTarget,
  selectedTileId,
  onClearSelection,
}: Props) {
  const [openVariant, setOpenVariant] = useState<null | "chow" | "pung" | "kong">(null);

  const { primaryButtons, variantCandidates, hint } = useMemo(() => {
    if (!pending) {
      return { primaryButtons: [] as PrimaryButton[], variantCandidates: [] as ClaimCandidate[], hint: "" };
    }
    if (pending.type === "active_turn") {
      const opts = new Set(pending.options);
      const buttons: PrimaryButton[] = [];
      if (opts.has("hu")) buttons.push({ key: "hu", label: "胡", tone: "hu" });
      if (opts.has("kong")) buttons.push({ key: "kong", label: "杠", tone: "default" });
      if (opts.has("flower")) buttons.push({ key: "flower", label: "花", tone: "default" });
      if (opts.has("discard"))
        buttons.push({
          key: "discard",
          label: "出",
          tone: "default",
          disabled: !selectedTileId,
        });
      return {
        primaryButtons: buttons,
        variantCandidates: [] as ClaimCandidate[],
        hint: selectedTileId ? "" : "选择一张手牌打出",
      };
    }
    if (pending.type === "claim_window") {
      const opts = new Set(pending.options);
      const buttons: PrimaryButton[] = [];
      if (opts.has("hu")) buttons.push({ key: "hu", label: "胡", tone: "hu" });
      if (opts.has("kong")) buttons.push({ key: "kong", label: "杠", tone: "default" });
      if (opts.has("pung")) buttons.push({ key: "pung", label: "碰", tone: "default" });
      if (opts.has("chow")) buttons.push({ key: "chow", label: "吃", tone: "default" });
      if (opts.has("pass")) buttons.push({ key: "pass", label: "过", tone: "pass" });
      return {
        primaryButtons: buttons,
        variantCandidates: [] as ClaimCandidate[],
        hint: "响应上家打出的牌",
      };
    }
    if (pending.type === "rob_kong_window") {
      const opts = new Set(pending.options);
      const buttons: PrimaryButton[] = [];
      if (opts.has("hu")) buttons.push({ key: "hu", label: "抢杠胡", tone: "hu" });
      if (opts.has("pass")) buttons.push({ key: "pass", label: "过", tone: "pass" });
      return {
        primaryButtons: buttons,
        variantCandidates: [] as ClaimCandidate[],
        hint: "对手补杠:可抢杠",
      };
    }
    if (pending.type === "opening_flowers") {
      const opts = new Set(pending.options);
      const buttons: PrimaryButton[] = [];
      if (opts.has("flower")) buttons.push({ key: "flower", label: "花", tone: "default" });
      if (opts.has("pass")) buttons.push({ key: "pass", label: "过", tone: "pass" });
      return {
        primaryButtons: buttons,
        variantCandidates: [] as ClaimCandidate[],
        hint: "开局补花",
      };
    }
    return { primaryButtons: [] as PrimaryButton[], variantCandidates: [] as ClaimCandidate[], hint: "" };
  }, [pending, selectedTileId]);

  void variantCandidates;

  const handlePrimary = (btn: PrimaryButton) => {
    if (btn.disabled) return;
    if (!pending) return;
    const key = btn.key;

    if (key === "discard") {
      if (!selectedTileId) return;
      onEmit({ action_type: "discard", tile_ids: [selectedTileId] });
      onClearSelection();
      return;
    }
    if (key === "flower") {
      // 选中一张花牌即发送
      if (selectedTileId) {
        onEmit({ action_type: "flower", tile_ids: [selectedTileId] });
        onClearSelection();
      } else {
        onEmit({ action_type: "flower", tile_ids: [] });
      }
      return;
    }
    if (key === "hu") {
      onEmit({ action_type: "hu", tile_ids: [] });
      return;
    }
    if (key === "pass") {
      onEmit({ action_type: "pass", tile_ids: [] });
      return;
    }
    if (key === "chow") {
      if (chowCandidates.length === 1) {
        onEmit({ action_type: "chow", tile_ids: chowCandidates[0].tileIds });
      } else {
        setOpenVariant(openVariant === "chow" ? null : "chow");
      }
      return;
    }
    if (key === "pung") {
      if (pungCandidates.length === 1) {
        onEmit({ action_type: "pung", tile_ids: pungCandidates[0].tileIds });
      } else {
        setOpenVariant(openVariant === "pung" ? null : "pung");
      }
      return;
    }
    if (key === "kong") {
      const list =
        pending.type === "claim_window"
          ? claimKongCandidates
          : selfKongCandidates;
      if (list.length === 1) {
        onEmit({ action_type: "kong", tile_ids: list[0].tileIds });
      } else if (list.length > 1) {
        setOpenVariant(openVariant === "kong" ? null : "kong");
      }
      return;
    }
  };

  const variantList: ClaimCandidate[] =
    openVariant === "chow"
      ? chowCandidates
      : openVariant === "pung"
        ? pungCandidates
        : openVariant === "kong"
          ? pending?.type === "claim_window"
            ? claimKongCandidates
            : selfKongCandidates
          : [];

  const activatableSkills = equippedSkills.filter((s) => s.can_activate_now);

  return (
    <>
      {hint ? <div className="turn-banner">{hint}</div> : null}
      <div className="action-dock">
        {activatableSkills.map((s) => (
          <button
            key={s.skill_id}
            type="button"
            className="action-btn skill"
            onClick={() => {
              if (s.interaction_kind === "select_target" && onSelectTarget) {
                onSelectTarget(s.skill_id);
              } else {
                onEmit({ action_type: `skill:${s.skill_id}`, tile_ids: [] });
              }
            }}
            title={s.summary}
          >
            {s.name}
          </button>
        ))}
        {primaryButtons.map((btn) => (
          <button
            key={btn.key}
            type="button"
            className={`action-btn ${btn.tone === "hu" ? "hu" : btn.tone === "pass" ? "pass" : ""}`}
            disabled={btn.disabled}
            onClick={() => handlePrimary(btn)}
          >
            {btn.label}
          </button>
        ))}
        {variantList.length > 1 && openVariant ? (
          <div className="action-variants">
            {variantList.map((c, i) => (
              <button
                key={i}
                type="button"
                className="variant-chip"
                onClick={() => {
                  onEmit({ action_type: c.action, tile_ids: c.tileIds });
                  setOpenVariant(null);
                }}
              >
                {c.previewKeys.map((k, j) => (
                  <span key={j} className="mini-tile">
                    {tileLabel(k)}
                  </span>
                ))}
              </button>
            ))}
          </div>
        ) : null}
      </div>
    </>
  );
}

interface PrimaryButton {
  key: "discard" | "flower" | "kong" | "hu" | "chow" | "pung" | "pass";
  label: string;
  tone: "default" | "hu" | "pass";
  disabled?: boolean;
}
