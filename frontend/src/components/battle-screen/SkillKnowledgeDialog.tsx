import type { SkillKnowledgeView } from '../../types/match';
import { MahjongTile } from './MahjongTile';

interface SkillKnowledgeDialogProps {
  knowledge: SkillKnowledgeView;
  onClose: () => void;
}

export function SkillKnowledgeDialog({ knowledge, onClose }: SkillKnowledgeDialogProps) {
  return (
    <div className="skill-activation-dialog skill-knowledge-dialog" role="dialog" aria-modal="true" aria-label={`${knowledge.title} · 情报`}>
      <div className="skill-activation-dialog__backdrop" onClick={onClose} />
      <section className="skill-activation-dialog__panel skill-activation-dialog__panel--azure skill-knowledge-dialog__panel">
        <div className="skill-activation-dialog__header">
          <div>
            <span className="skill-activation-dialog__eyebrow">技能情报</span>
            <h2>{knowledge.title}</h2>
            <p>{knowledge.detail}</p>
          </div>
          <button type="button" className="skill-activation-dialog__close" aria-label="关闭情报浮窗" onClick={onClose}>
            ×
          </button>
        </div>

        <div className="skill-activation-dialog__group">
          <span className="skill-activation-dialog__label">侦察目标</span>
          <div className="skill-knowledge-dialog__target">{knowledge.targetName}</div>
        </div>

        <div className="skill-activation-dialog__group">
          <span className="skill-activation-dialog__label">已查看牌面</span>
          <div className="skill-activation-dialog__preview-grid" aria-label={`${knowledge.skillName} 查看到的牌`}>
            {knowledge.tileCodes.map((tileCode, index) => (
              <div key={`${knowledge.key}-${tileCode}-${index}`} className="skill-activation-dialog__preview-tile">
                <MahjongTile code={tileCode} variant="discard" className="skill-activation-dialog__preview-tile-face" />
                <span>第 {index + 1} 张</span>
              </div>
            ))}
          </div>
        </div>

        <div className="skill-activation-dialog__footer">
          <button type="button" className="skill-activation-dialog__secondary" onClick={onClose}>
            收起
          </button>
        </div>
      </section>
    </div>
  );
}
