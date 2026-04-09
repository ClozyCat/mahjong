import type { SkillActivationView } from '../../types/match';
import { MahjongTile } from './MahjongTile';

interface SkillActivationDialogProps {
  activation: SkillActivationView;
  onTargetSelect: (seatIndex: number) => void;
  onTileSelect: (tileId: string) => void;
  onMeldSelect: (meldIndex: number) => void;
  onConfirm: () => void;
  onClose: () => void;
}

export function SkillActivationDialog({
  activation,
  onTargetSelect,
  onTileSelect,
  onMeldSelect,
  onConfirm,
  onClose,
}: SkillActivationDialogProps) {
  return (
    <div className="skill-activation-dialog" role="dialog" aria-modal="true" aria-label={activation.title}>
      <div className="skill-activation-dialog__backdrop" onClick={onClose} />
      <section className={`skill-activation-dialog__panel skill-activation-dialog__panel--${activation.skill.tone}`}>
        <div className="skill-activation-dialog__header">
          <div>
            <span className="skill-activation-dialog__eyebrow">
              {activation.skill.rarityLabel} · {activation.skill.typeLabel}
            </span>
            <h2>{activation.title}</h2>
            <p>{activation.description}</p>
          </div>
          <button type="button" className="skill-activation-dialog__close" aria-label="关闭技能面板" onClick={onClose}>
            ×
          </button>
        </div>

        {activation.targetChoices?.length ? (
          <div className="skill-activation-dialog__group">
            <span className="skill-activation-dialog__label">选择目标牌手</span>
            <div className="skill-activation-dialog__choice-grid">
              {activation.targetChoices.map((choice) => (
                <button
                  key={choice.id}
                  type="button"
                  className={`skill-activation-dialog__choice ${choice.selected ? 'skill-activation-dialog__choice--selected' : ''}`.trim()}
                  onClick={() => onTargetSelect(Number(choice.id))}
                >
                  <strong>{choice.label}</strong>
                  {choice.description ? <span>{choice.description}</span> : null}
                </button>
              ))}
            </div>
          </div>
        ) : null}

        {activation.handChoices?.length ? (
          <div className="skill-activation-dialog__group">
            <span className="skill-activation-dialog__label">选择一张手牌</span>
            <div className="skill-activation-dialog__tile-grid">
              {activation.handChoices.map((choice) => (
                <button
                  key={choice.tileId}
                  type="button"
                  className={`skill-activation-dialog__tile-choice ${choice.selected ? 'skill-activation-dialog__tile-choice--selected' : ''}`.trim()}
                  onClick={() => onTileSelect(choice.tileId)}
                >
                  <MahjongTile code={choice.code} variant="hand" className="skill-activation-dialog__tile" />
                  <span>{choice.label}</span>
                </button>
              ))}
            </div>
          </div>
        ) : null}

        {activation.meldChoices?.length ? (
          <div className="skill-activation-dialog__group">
            <span className="skill-activation-dialog__label">选择要回收的副露</span>
            <div className="skill-activation-dialog__meld-list">
              {activation.meldChoices.map((choice) => (
                <button
                  key={`${choice.label}-${choice.index}`}
                  type="button"
                  className={`skill-activation-dialog__meld-choice ${choice.selected ? 'skill-activation-dialog__meld-choice--selected' : ''}`.trim()}
                  onClick={() => onMeldSelect(choice.index)}
                >
                  <strong>{choice.label}</strong>
                  <div className="skill-activation-dialog__meld-tiles">
                    {choice.tiles.map((tile, index) => (
                      <MahjongTile key={`${choice.index}-${tile}-${index}`} code={tile} variant="discard" className="skill-activation-dialog__meld-tile" />
                    ))}
                  </div>
                </button>
              ))}
            </div>
          </div>
        ) : null}

        {activation.previewTiles?.length ? (
          <div className="skill-activation-dialog__group">
            <span className="skill-activation-dialog__label">尾牌预览位</span>
            <div className="skill-activation-dialog__preview-grid">
              {activation.previewTiles.map((tile) => (
                <div key={tile.key} className="skill-activation-dialog__preview-tile">
                  <MahjongTile code={tile.code} variant="discard" className="skill-activation-dialog__preview-tile-face" />
                  <span>{tile.label}</span>
                </div>
              ))}
            </div>
          </div>
        ) : null}

        <div className="skill-activation-dialog__footer">
          <button type="button" className="skill-activation-dialog__secondary" onClick={onClose}>
            取消
          </button>
          <button type="button" className="skill-activation-dialog__primary" disabled={!activation.canConfirm} onClick={onConfirm}>
            {activation.confirmLabel}
          </button>
        </div>
      </section>
    </div>
  );
}
