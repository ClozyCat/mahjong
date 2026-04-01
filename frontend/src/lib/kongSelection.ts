import type { BackendActionType, SessionState } from '../types/match';

type MeldAction = Extract<BackendActionType, 'kong' | 'chow' | 'pung'>;

function normalizeTileKey(tileKey: string | null | undefined) {
  const normalized = tileKey?.trim().toLowerCase() ?? '';
  const match = normalized.match(/^([wbcmpt])([1-9])$/);
  if (!match) {
    return normalized;
  }

  const [, suit, rank] = match;
  return `${normalizeSuit(suit)}${rank}`;
}

function normalizeSuit(suit: string) {
  if (suit === 'm') {
    return 'w';
  }
  if (suit === 'p') {
    return 'b';
  }
  if (suit === 'c') {
    return 't';
  }
  return suit;
}

function parseSuitedTileKey(tileKey: string) {
  const match = normalizeTileKey(tileKey).match(/^([wtb])([1-9])$/);
  if (!match) {
    return null;
  }

  const [, suit, rankText] = match;
  return {
    suit: normalizeSuit(suit),
    rank: Number(rankText),
  };
}

function chooseCombinations(values: string[], size: number): string[][] {
  if (size === 0) {
    return [[]];
  }

  if (values.length < size) {
    return [];
  }

  const results: string[][] = [];

  function visit(startIndex: number, current: string[]) {
    if (current.length === size) {
      results.push([...current]);
      return;
    }

    for (let index = startIndex; index <= values.length - (size - current.length); index += 1) {
      current.push(values[index]);
      visit(index + 1, current);
      current.pop();
    }
  }

  visit(0, []);
  return results;
}

function getLocalPrivatePlayer(state: SessionState) {
  const snapshot = state.roomSnapshot?.payload;
  const localSeat = snapshot?.local_seat;

  if (typeof localSeat !== 'number') {
    return null;
  }

  return snapshot?.private_state?.players.find((player) => player.seat_index === localSeat) ?? null;
}

function getLocalSeat(state: SessionState) {
  return state.roomSnapshot?.payload.local_seat ?? null;
}

function getLocalActiveTurn(state: SessionState) {
  const snapshot = state.roomSnapshot?.payload;
  const localSeat = getLocalSeat(state);
  const pendingAction = snapshot?.private_state?.pending_action;

  if (
    typeof localSeat !== 'number' ||
    snapshot?.phase !== 'playing' ||
    pendingAction?.type !== 'active_turn' ||
    pendingAction.seat_index !== localSeat
  ) {
    return null;
  }

  return pendingAction;
}

function getPromptAllowsAction(state: SessionState, action: MeldAction) {
  const latestPrompt = state.latestActionPrompt?.payload.options ?? [];
  if (latestPrompt.includes(action)) {
    return true;
  }

  const pendingAction = state.roomSnapshot?.payload.private_state?.pending_action;
  if (!pendingAction || !('options' in pendingAction) || !Array.isArray(pendingAction.options)) {
    return false;
  }

  return pendingAction.options.includes(action);
}

function getConcealedByKey(state: SessionState) {
  const localPlayer = getLocalPrivatePlayer(state);
  const concealedTiles = localPlayer?.concealed_tiles ?? [];
  const concealedByKey = new Map<string, string[]>();

  for (const tile of concealedTiles) {
    const tileKey = normalizeTileKey(tile.tile_key);
    const existing = concealedByKey.get(tileKey) ?? [];
    existing.push(tile.tile_id);
    concealedByKey.set(tileKey, existing);
  }

  return { localPlayer, concealedTiles, concealedByKey };
}

export function isFlowerTileKey(tileKey: string | null | undefined) {
  const normalized = normalizeTileKey(tileKey);
  return /^f\d+$/.test(normalized) || normalized.startsWith('flower') || normalized.startsWith('season');
}

function pushUniqueGroup(groups: string[][], tileIds: string[]) {
  const normalized = [...new Set(tileIds)];
  if (normalized.length === 0) {
    return;
  }

  const signature = normalized.slice().sort().join('|');
  if (!groups.some((group) => group.slice().sort().join('|') === signature)) {
    groups.push(normalized);
  }
}

function collectActiveTurnKongCandidateGroups(
  localPlayer: ReturnType<typeof getLocalPrivatePlayer>,
  concealedByKey: Map<string, string[]>,
) {
  const groups: string[][] = [];

  for (const tileIds of concealedByKey.values()) {
    if (tileIds.length >= 4) {
      pushUniqueGroup(groups, tileIds.slice(0, 4));
    }
  }

  for (const meld of localPlayer?.melds ?? []) {
    if (meld.length !== 3) {
      continue;
    }

    const meldKey = normalizeTileKey(meld[0]);
    if (!meld.every((tile) => normalizeTileKey(tile) === meldKey)) {
      continue;
    }

    const matchingConcealed = concealedByKey.get(meldKey);
    if (matchingConcealed?.length) {
      pushUniqueGroup(groups, [matchingConcealed[0]]);
    }
  }

  return groups;
}

export function getLocalTurnKongCandidateGroups(state: SessionState): string[][] {
  const activeTurn = getLocalActiveTurn(state);
  const { localPlayer, concealedTiles, concealedByKey } = getConcealedByKey(state);

  if (!activeTurn || concealedTiles.length === 0) {
    return [];
  }

  return collectActiveTurnKongCandidateGroups(localPlayer, concealedByKey);
}

export function getLocalTurnKongPromptSignature(state: SessionState): string | null {
  const snapshot = state.roomSnapshot?.payload;
  const activeTurn = getLocalActiveTurn(state);
  const groups = getLocalTurnKongCandidateGroups(state);

  if (!snapshot?.private_state || !activeTurn || groups.length === 0) {
    return null;
  }

  const groupSignature = groups
    .map((group) => group.slice().sort().join('|'))
    .sort()
    .join('||');

  return [
    'turn-kong',
    snapshot.private_state.round_id,
    activeTurn.seat_index,
    activeTurn.deadline_at,
    activeTurn.drawn_tile_id ?? '',
    groupSignature,
  ].join(':');
}

export function getActionCandidateGroups(state: SessionState, action: MeldAction): string[][] {
  if (!getPromptAllowsAction(state, action)) {
    return [];
  }

  const snapshot = state.roomSnapshot?.payload;
  const privateState = snapshot?.private_state;
  const { localPlayer, concealedTiles, concealedByKey } = getConcealedByKey(state);

  if (!privateState || concealedTiles.length === 0) {
    return [];
  }

  const groups: string[][] = [];

  if (action === 'kong' && privateState.pending_action?.type === 'active_turn') {
    for (const group of collectActiveTurnKongCandidateGroups(localPlayer, concealedByKey)) {
      pushUniqueGroup(groups, group);
    }
  }

  if (privateState.pending_action?.type === 'claim_window') {
    const discardKey = normalizeTileKey(privateState.last_discard);

    if (action === 'kong') {
      const matchingConcealed = concealedByKey.get(discardKey);
      if (matchingConcealed && matchingConcealed.length >= 3) {
        for (const combo of chooseCombinations(matchingConcealed, 3)) {
          pushUniqueGroup(groups, combo);
        }
      }
    }

    if (action === 'pung') {
      const matchingConcealed = concealedByKey.get(discardKey);
      if (matchingConcealed && matchingConcealed.length >= 2) {
        for (const combo of chooseCombinations(matchingConcealed, 2)) {
          pushUniqueGroup(groups, combo);
        }
      }
    }

    if (action === 'chow') {
      const discardTile = parseSuitedTileKey(discardKey);
      if (discardTile) {
        const rankPairs: Array<[number, number]> = [
          [discardTile.rank - 2, discardTile.rank - 1],
          [discardTile.rank - 1, discardTile.rank + 1],
          [discardTile.rank + 1, discardTile.rank + 2],
        ];

        for (const [leftRank, rightRank] of rankPairs) {
          if (leftRank < 1 || rightRank > 9) {
            continue;
          }

          const leftTiles = concealedByKey.get(`${discardTile.suit}${leftRank}`) ?? [];
          const rightTiles = concealedByKey.get(`${discardTile.suit}${rightRank}`) ?? [];

          if (leftTiles.length === 0 || rightTiles.length === 0) {
            continue;
          }

          for (const leftTileId of leftTiles) {
            for (const rightTileId of rightTiles) {
              pushUniqueGroup(groups, [leftTileId, rightTileId]);
            }
          }
        }
      }
    }
  }

  return groups;
}

export function getActionCandidateTileIds(state: SessionState, action: MeldAction): string[] {
  return [...new Set(getActionCandidateGroups(state, action).flat())];
}

export function getFlowerCandidateTileIds(state: SessionState): string[] {
  const localPlayer = getLocalPrivatePlayer(state);
  const concealedTiles = localPlayer?.concealed_tiles ?? [];

  return concealedTiles.filter((tile) => isFlowerTileKey(tile.tile_key)).map((tile) => tile.tile_id);
}

export function getMatchingActionGroup(selectedTileIds: string[], candidateGroups: string[][]): string[] | null {
  const normalizedSelection = [...new Set(selectedTileIds)].sort();

  if (normalizedSelection.length === 0) {
    return null;
  }

  for (const group of candidateGroups) {
    const normalizedGroup = [...new Set(group)].sort();
    if (
      normalizedGroup.length === normalizedSelection.length &&
      normalizedGroup.every((tileId, index) => tileId === normalizedSelection[index])
    ) {
      return group;
    }
  }

  return null;
}

export function getKongCandidateGroups(state: SessionState): string[][] {
  return getActionCandidateGroups(state, 'kong');
}

export function getKongCandidateTileIds(state: SessionState): string[] {
  return getActionCandidateTileIds(state, 'kong');
}

export function getMatchingKongGroup(selectedTileIds: string[], candidateGroups: string[][]): string[] | null {
  return getMatchingActionGroup(selectedTileIds, candidateGroups);
}
