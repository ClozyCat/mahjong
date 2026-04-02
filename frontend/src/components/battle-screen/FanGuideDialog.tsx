import { useEffect, useState } from 'react';
import { createPortal } from 'react-dom';

import { FAN_GUIDE_ENTRIES } from './fanGuide';

interface FanGuideDialogProps {
  isOpen: boolean;
  onClose: () => void;
}

const FAN_GUIDE_PAGE_SIZE = 6;

export function FanGuideDialog({ isOpen, onClose }: FanGuideDialogProps) {
  const [page, setPage] = useState(0);
  const pageCount = Math.max(1, Math.ceil(FAN_GUIDE_ENTRIES.length / FAN_GUIDE_PAGE_SIZE));
  const currentPage = Math.min(page, pageCount - 1);
  const pageStart = currentPage * FAN_GUIDE_PAGE_SIZE;
  const visibleEntries = FAN_GUIDE_ENTRIES.slice(pageStart, pageStart + FAN_GUIDE_PAGE_SIZE);

  useEffect(() => {
    if (!isOpen) {
      return;
    }

    setPage(0);
  }, [isOpen]);

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
          </div>
          <button type="button" className="fan-guide__close" aria-label="关闭番种说明" onClick={onClose}>
            关闭
          </button>
        </header>

        <div className="fan-guide__grid" aria-live="polite">
          {visibleEntries.map((entry) => (
            <article key={entry.fanKey} className="fan-guide__card">
              <div className="fan-guide__card-head">
                <div className="fan-guide__card-title">
                  <strong>{entry.label}</strong>
                </div>
                <span className="fan-guide__fan-pill">{entry.fanValue} 番</span>
              </div>
              <p className="fan-guide__card-copy">{entry.intro}</p>
              <p className="fan-guide__example">{entry.example}</p>
            </article>
          ))}
        </div>

        <footer className="fan-guide__pagination" role="group" aria-label="番种说明分页">
          <span className="fan-guide__pagination-status">
            第 {currentPage + 1} / {pageCount} 页
          </span>
          <div className="fan-guide__pagination-actions">
            <button
              type="button"
              className="fan-guide__pagination-button"
              onClick={() => setPage((current) => Math.max(0, current - 1))}
              disabled={currentPage === 0}
            >
              上一页
            </button>
            <button
              type="button"
              className="fan-guide__pagination-button"
              onClick={() => setPage((current) => Math.min(pageCount - 1, current + 1))}
              disabled={currentPage >= pageCount - 1}
            >
              下一页
            </button>
          </div>
        </footer>
      </section>
    </div>,
    document.body,
  );
}
