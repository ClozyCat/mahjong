import { createPortal } from 'react-dom';

import { FAN_GUIDE_ENTRIES } from './fanGuide';
import { FanGuideCard } from './FanGuideCard';

interface FanGuideDialogProps {
  isOpen: boolean;
  onClose: () => void;
}

export function FanGuideDialog({ isOpen, onClose }: FanGuideDialogProps) {
  if (!isOpen) {
    return null;
  }

  if (typeof document === 'undefined') {
    return null;
  }

  return createPortal(
    <div className="fan-guide__backdrop" role="presentation">
      <section className="fan-guide__dialog" role="dialog" aria-modal="true" aria-label="国标麻将番种说明">
        <header className="fan-guide__header">
          <div className="fan-guide__title-block">
            <span className="fan-guide__eyebrow">国标麻将番种说明</span>
            <p className="fan-guide__hint">按番值顺序排列，下滑查看全部番种。</p>
          </div>
          <button type="button" className="fan-guide__close" aria-label="关闭番种说明" onClick={onClose}>
            关闭
          </button>
        </header>

        <div className="fan-guide__content" aria-label="番种说明列表" aria-live="polite" tabIndex={0}>
          <div className="fan-guide__grid">
            {FAN_GUIDE_ENTRIES.map((entry) => (
              <FanGuideCard key={entry.fanKey} entry={entry} />
            ))}
          </div>
        </div>
      </section>
    </div>,
    document.body,
  );
}
