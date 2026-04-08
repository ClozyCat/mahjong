import { useEffect, useState } from 'react';

import type { SkillSelectionView } from '../../types/match';

interface SkillSelectionOverlayProps {
  selection: SkillSelectionView;
  onSelect: (skillId: string) => void;
  onDecline: () => void;
}

export function SkillSelectionOverlay({ selection, onSelect, onDecline }: SkillSelectionOverlayProps) {
  const [remainingSeconds, setRemainingSeconds] = useState<number | null>(null);

  useEffect(() => {
    const update = () => {
      const nextRemaining = Math.max(0, Math.ceil((new Date(selection.deadlineAt).getTime() - Date.now()) / 1000));
      setRemainingSeconds(nextRemaining);
    };

    update();
    const timer = window.setInterval(update, 250);
    return () => window.clearInterval(timer);
  }, [selection.deadlineAt]);

  return (
    <div className="skill-selection-overlay" role="dialog" aria-modal="true" aria-label={selection.title}>
      <div className="skill-selection-overlay__backdrop" />
      <section className="skill-selection-overlay__panel">
        <div className="skill-selection-overlay__header">
          <div>
            <span className="skill-selection-overlay__eyebrow">{selection.cycleLabel}</span>
            <h2>{selection.title}</h2>
            <p>{selection.detail}</p>
          </div>
          <div className={`skill-selection-overlay__countdown ${remainingSeconds !== null && remainingSeconds <= 5 ? 'skill-selection-overlay__countdown--critical' : ''}`.trim()}>
            <span>倒计时</span>
            <strong>{remainingSeconds ?? '--'}s</strong>
          </div>
        </div>
        <div className="skill-selection-overlay__options" aria-label="可选技能列表">
          {selection.options.map((skill) => (
            <article
              key={`${selection.cycleKey}-${skill.skillId}-${skill.rarity}`}
              className={`skill-card skill-card--${skill.tone}`}
            >
              <div className="skill-card__header">
                <span className={`skill-card__rarity skill-card__rarity--${skill.tone}`}>{skill.rarityLabel}</span>
                <span className="skill-card__type">{skill.typeLabel}</span>
              </div>
              <strong className="skill-card__name">{skill.name}</strong>
              <p className="skill-card__summary">{skill.summary}</p>
              <p className="skill-card__detail">{skill.detail}</p>
              <div className="skill-card__meta">
                <span>持续 {skill.remainingRounds} 局</span>
                {skill.type === 'active' ? <span>本局可发动 {skill.remainingActivationsThisRound} 次</span> : null}
              </div>
              {skill.interactionHint ? <p className="skill-card__hint">{skill.interactionHint}</p> : null}
              <div className="skill-card__tags">
                {skill.tags.map((tag) => (
                  <span key={`${skill.skillId}-${tag}`} className="skill-card__tag">
                    {tag}
                  </span>
                ))}
              </div>
              <button type="button" className="skill-card__select" onClick={() => onSelect(skill.skillId)}>
                选择此技能
              </button>
            </article>
          ))}
        </div>
        <div className="skill-selection-overlay__footer">
          <button type="button" className="skill-selection-overlay__decline" onClick={onDecline}>
            不需要技能
          </button>
        </div>
      </section>
    </div>
  );
}
