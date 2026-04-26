import type { RefObject } from 'react';

import type { PlayerView } from '../../../types/match';
import type { TableLayoutProfile } from './layoutProfiles';

export type TableStagePlayer = Pick<PlayerView, 'seat' | 'name' | 'melds'> &
  Partial<Omit<PlayerView, 'seat' | 'name' | 'melds'>>;

export type TableFxMode = 'fullFx' | 'lowFx';

export interface BattleViewportMetrics {
  containerRef: RefObject<HTMLElement | null>;
  width: number;
  height: number;
  aspectRatio: number;
  isSmallScreen: boolean;
  effectMode: TableFxMode;
  layoutProfile: TableLayoutProfile;
}
