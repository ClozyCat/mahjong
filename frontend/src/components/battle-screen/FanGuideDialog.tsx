import { createPortal } from 'react-dom';
import { useEffect, useRef, useState, useMemo } from 'react';

import { FAN_GUIDE_ENTRIES } from './fanGuide';
import { FanGuideCard, getFanColor } from './FanGuideCard';

interface FanGuideDialogProps {
  isOpen: boolean;
  onClose: () => void;
}

/**
 * LazySection component to prevent rendering of content that is not yet near the viewport.
 * This significantly improves the initial opening speed and scrolling performance for long lists.
 */
function LazySection({ children, label, value }: { children: React.ReactNode; label: string; value: number }) {
  const [isVisible, setIsVisible] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const observer = new IntersectionObserver(
      ([entry]) => {
        if (entry.isIntersecting) {
          setIsVisible(true);
          observer.disconnect();
        }
      },
      { rootMargin: '300px' },
    );

    if (ref.current) {
      observer.observe(ref.current);
    }

    return () => observer.disconnect();
  }, []);

  return (
    <section ref={ref} className="fan-guide__section" id={`fan-section-${value}`}>
      <h3 className="fan-guide__section-title">
        <span>{label}</span>
      </h3>
      <div className="fan-guide__grid">
        {isVisible ? children : <div className="fan-guide__loading-placeholder" />}
      </div>
    </section>
  );
}

export function FanGuideDialog({ isOpen, onClose }: FanGuideDialogProps) {
  const scrollContainerRef = useRef<HTMLDivElement>(null);
  const sidebarRef = useRef<HTMLElement>(null);
  const [activeTab, setActiveTab] = useState<number | null>(null);
  const [searchQuery, setSearchQuery] = useState('');

  // Filter entries based on search query
  const filteredEntries = useMemo(() => {
    if (!searchQuery.trim()) return FAN_GUIDE_ENTRIES;
    const query = searchQuery.toLowerCase().trim();
    return FAN_GUIDE_ENTRIES.filter((entry) => 
      entry.label.toLowerCase().includes(query)
    );
  }, [searchQuery]);

  // Group entries by fan value
  const groupedEntries = useMemo(() => {
    const groups = new Map<number, typeof FAN_GUIDE_ENTRIES>();
    filteredEntries.forEach((entry) => {
      const list = groups.get(entry.fanValue) || [];
      list.push(entry);
      groups.set(entry.fanValue, list);
    });
    return Array.from(groups.entries())
      .map(([value, entries]) => ({ value, entries }))
      .sort((a, b) => a.value - b.value);
  }, [filteredEntries]);

  useEffect(() => {
    if (isOpen && groupedEntries.length > 0) {
      // If we are searching, we might want to keep the current tab or set to the first result
      if (!activeTab || !groupedEntries.some(g => g.value === activeTab)) {
        setActiveTab(groupedEntries[0].value);
      }
    }
  }, [isOpen, groupedEntries, activeTab]);

  // Observer to track which section is currently at the top of the viewport
  useEffect(() => {
    if (!isOpen || !scrollContainerRef.current) return;

    const observer = new IntersectionObserver(
      (entries) => {
        entries.forEach((entry) => {
          if (entry.isIntersecting) {
            const id = entry.target.id;
            const value = parseInt(id.replace('fan-section-', ''), 10);
            if (!isNaN(value)) {
              setActiveTab(value);
            }
          }
        });
      },
      {
        root: scrollContainerRef.current,
        rootMargin: '0px 0px -80% 0px',
        threshold: 0,
      }
    );

    const sections = scrollContainerRef.current.querySelectorAll('.fan-guide__section');
    sections.forEach((section) => observer.observe(section));

    return () => observer.disconnect();
  }, [isOpen, groupedEntries]);

  // Scroll active sidebar item into view
  useEffect(() => {
    if (activeTab === null || !sidebarRef.current) return;
    
    // Small delay to ensure the DOM state (like the active class) is fully applied
    const timeoutId = setTimeout(() => {
      const activeItem = sidebarRef.current?.querySelector('.fan-guide__nav-item--active');
      if (activeItem) {
        activeItem.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
      }
    }, 50);
    
    return () => clearTimeout(timeoutId);
  }, [activeTab]);

  const scrollToSection = (value: number) => {
    const element = document.getElementById(`fan-section-${value}`);
    if (element && scrollContainerRef.current) {
      element.scrollIntoView({ behavior: 'smooth' });
      setActiveTab(value);
    }
  };

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
            <span className="fan-guide__eyebrow">番种说明</span>
            <p className="fan-guide__hint">按番值分类排列，侧边栏可快速跳转。</p>
          </div>

          <div className="fan-guide__search">
            <div className="fan-guide__search-icon">
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round" strokeLinejoin="round">
                <circle cx="11" cy="11" r="8"></circle>
                <line x1="21" y1="21" x2="16.65" y2="16.65"></line>
              </svg>
            </div>
            <input 
              type="text" 
              className="fan-guide__search-input" 
              placeholder="搜索番种名称..." 
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
            />
            {searchQuery && (
              <button 
                type="button" 
                className="fan-guide__search-clear" 
                onClick={() => setSearchQuery('')}
                aria-label="清空搜索"
              >
                <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="4" strokeLinecap="round" strokeLinejoin="round">
                  <line x1="18" y1="6" x2="6" y2="18"></line>
                  <line x1="6" y1="6" x2="18" y2="18"></line>
                </svg>
              </button>
            )}
          </div>

          <button type="button" className="fan-guide__close" aria-label="关闭番种说明" onClick={onClose}>
            关闭
          </button>
        </header>

        <div className="fan-guide__layout">
          <nav className="fan-guide__sidebar" ref={sidebarRef}>
            {groupedEntries.map(({ value }) => (
              <button
                key={value}
                type="button"
                className={`fan-guide__nav-item ${activeTab === value ? 'fan-guide__nav-item--active' : ''}`}
                style={activeTab === value ? { '--nav-active-bg': getFanColor(value) } as any : {}}
                onClick={() => scrollToSection(value)}
              >
                <span className="fan-guide__nav-value">{value}</span>
                <span className="fan-guide__nav-unit">番</span>
              </button>
            ))}
          </nav>

          <div
            className="fan-guide__content"
            aria-label="番种说明列表"
            aria-live="polite"
            tabIndex={0}
            ref={scrollContainerRef}
          >
            {groupedEntries.length > 0 ? (
              groupedEntries.map(({ value, entries }) => (
                <LazySection key={value} value={value} label={`${value} 番`}>
                  {entries.map((entry) => (
                    <FanGuideCard 
                      key={entry.fanKey} 
                      entry={entry} 
                    />
                  ))}
                </LazySection>
              ))
            ) : (
              <div className="fan-guide__empty-state">
                <p>未找到匹配“{searchQuery}”的番种</p>
                <button type="button" className="fan-guide__clear-search" onClick={() => setSearchQuery('')}>
                  清除搜索
                </button>
              </div>
            )}
            <footer className="fan-guide__footer">
              <p>到底了，祝阁下每局都能和得漂亮！</p>
            </footer>
          </div>
        </div>
      </section>
    </div>,
    document.body,
  );
}
