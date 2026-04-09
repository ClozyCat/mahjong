import { useEffect, useState } from 'react';

import type { SkillSelectionView } from '../../types/match';

interface SkillSelectionOverlayProps {
  selection: SkillSelectionView;
  onSelect: (skillId: string) => void;
  onDecline: () => void;
}

const SKILL_POSITION_LABELS = ['左签', '右签'];

export function SkillSelectionOverlay({ selection, onSelect, onDecline }: SkillSelectionOverlayProps) {
  const [remainingSeconds, setRemainingSeconds] = useState<number | null>(null);
  const displayTitle = getSelectionDisplayTitle(selection.title, selection.cycleLabel);

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
          <div className="skill-selection-overlay__header-bar">
            <span className="skill-selection-overlay__eyebrow">{selection.cycleLabel}</span>
            <div
              className={`skill-selection-overlay__countdown ${
                remainingSeconds !== null && remainingSeconds <= 5 ? 'skill-selection-overlay__countdown--critical' : ''
              }`.trim()}
            >
              <span>剩余</span>
              <strong>{remainingSeconds ?? '--'}s</strong>
            </div>
          </div>
          <div className="skill-selection-overlay__title-block">
            <h2>{displayTitle}</h2>
            {selection.detail ? <p>{selection.detail}</p> : null}
          </div>
        </div>
        <div className="skill-selection-overlay__body">
          <div className="skill-selection-overlay__options" aria-label="可选技能列表">
            {selection.options.map((skill, index) => (
              <article
                key={`${selection.cycleKey}-${skill.skillId}-${skill.rarity}`}
                className={`skill-card skill-card--${skill.tone}`.trim()}
              >
                <div className="skill-card__header">
                  <span className="skill-card__seat">{SKILL_POSITION_LABELS[index] ?? `第${index + 1}签`}</span>
                  <div className="skill-card__badges">
                    <span className={`skill-card__rarity skill-card__rarity--${skill.tone}`}>{skill.rarityLabel}</span>
                    <span className="skill-card__type">{skill.typeLabel}</span>
                  </div>
                </div>
                <div className="skill-card__title-block">
                  <strong className="skill-card__name">{skill.name}</strong>
                  <span className="skill-card__cycle">持续 {skill.remainingRounds} 局</span>
                </div>
                <p className="skill-card__summary">{skill.summary}</p>
                <p className="skill-card__detail">{skill.detail}</p>
                <div className="skill-card__meta">
                  {skill.type === 'active' ? <span>本局可发动 {skill.remainingActivationsThisRound} 次</span> : <span>整局自动生效</span>}
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
        </div>
        <div className="skill-selection-overlay__footer">
          <span className="skill-selection-overlay__footer-line" aria-hidden="true" />
          <button type="button" className="skill-selection-overlay__decline" onClick={onDecline}>
            不需要技能
          </button>
          <span className="skill-selection-overlay__footer-line" aria-hidden="true" />
        </div>
      </section>
    </div>
  );
}

function getSelectionDisplayTitle(title: string, cycleLabel: string) {
  const prefixes = [
    `${cycleLabel} · `,
    `${cycleLabel}·`,
    `${cycleLabel} | `,
    `${cycleLabel}|`,
    `${cycleLabel} ｜ `,
    `${cycleLabel}｜`,
    `${cycleLabel} - `,
    `${cycleLabel}-`,
  ];

  for (const prefix of prefixes) {
    if (title.startsWith(prefix)) {
      return title.slice(prefix.length).trim();
    }
  }

  return title;
}
