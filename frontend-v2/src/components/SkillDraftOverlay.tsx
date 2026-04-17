import type { SkillDraftState, SkillOption } from "../types/protocol";

interface Props {
  draft: SkillDraftState;
  onSelect: (skill: SkillOption) => void;
  onDecline: () => void;
}

export function SkillDraftOverlay({ draft, onSelect, onDecline }: Props) {
  return (
    <div className="skill-overlay">
      <div className="skill-title">{draft.title}</div>
      <div className="skill-detail">{draft.detail}</div>
      <div className="skill-cards">
        {draft.options.map((opt) => (
          <div
            key={opt.skill_id}
            className="skill-card"
            onClick={() => onSelect(opt)}
          >
            <div className="serial">{opt.serial ?? opt.skill_id}</div>
            <div className="name">{opt.name}</div>
            <div className="rarity">
              {opt.rarity_label} · {opt.type_label}
            </div>
            <div className="summary">{opt.summary || opt.detail}</div>
            {opt.tags?.length ? (
              <div className="tag-list">
                {opt.tags.map((t) => (
                  <span className="tag" key={t}>
                    {t}
                  </span>
                ))}
              </div>
            ) : null}
          </div>
        ))}
      </div>
      <div className="skill-actions-row">
        <button type="button" className="btn-ghost" onClick={onDecline}>
          弃选
        </button>
      </div>
    </div>
  );
}
