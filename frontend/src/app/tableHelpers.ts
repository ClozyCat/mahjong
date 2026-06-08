import type { InviteDialogUser, PlayerInviteStatus } from '../components/battle-screen/PlayerInviteDialog';
import { createClaimCandidates } from '../lib/matchViewModel';
import { titleForPoints } from '../lib/systemBroadcastCopy';
import type {
  BackendActionType,
  BattleActionId,
  CreateTableResponse,
  PublicUser,
  RoomSnapshotMessage,
  SessionState,
  TableInvite,
} from '../types/match';
import { CLAIM_RESPONSE_ACTION_IDS, TABLE_SEAT_CAPACITY } from './config';

function hasClaimAction(options: BackendActionType[]) {
  return CLAIM_RESPONSE_ACTION_IDS.some((actionId) => options.includes(actionId));
}

export function getClaimSelectionSignature(state: SessionState) {
  const pendingAction = state.roomSnapshot?.payload.private_state?.pending_action;

  if (pendingAction?.type === 'claim_window' && Array.isArray(pendingAction.options)) {
    const options = pendingAction.options
      .filter((option): option is BackendActionType => typeof option === 'string')
      .filter((option): option is (typeof CLAIM_RESPONSE_ACTION_IDS)[number] =>
        CLAIM_RESPONSE_ACTION_IDS.includes(option as (typeof CLAIM_RESPONSE_ACTION_IDS)[number]),
      );
    return options.length > 0 ? `claim:${pendingAction.deadline_at}:${options.slice().sort().join(',')}` : null;
  }

  const promptOptions = (state.latestActionPrompt?.payload.options ?? []).filter(
    (option): option is BackendActionType =>
      CLAIM_RESPONSE_ACTION_IDS.includes(option as (typeof CLAIM_RESPONSE_ACTION_IDS)[number]) || option === 'pass',
  );
  if (promptOptions.includes('pass') && hasClaimAction(promptOptions)) {
    const options = promptOptions.filter((option): option is (typeof CLAIM_RESPONSE_ACTION_IDS)[number] =>
      CLAIM_RESPONSE_ACTION_IDS.includes(option as (typeof CLAIM_RESPONSE_ACTION_IDS)[number]),
    );
    return options.length > 0 ? `claim:${state.latestActionPrompt?.payload.deadline_at ?? ''}:${options.slice().sort().join(',')}` : null;
  }

  return null;
}

export function canUseClaimMultiSelect(state: SessionState) {
  return getClaimSelectionSignature(state) !== null;
}

export function getDefaultClaimCandidateSelection(state: SessionState) {
  const firstCandidate = createClaimCandidates(state)[0];

  if (!firstCandidate) {
    return null;
  }

  return {
    actionId: firstCandidate.actionId,
    tileIds: firstCandidate.tileIds,
  };
}

export function canQuickDiscard(state: SessionState, hasLocalTurnKongPrompt: boolean) {
  if (state.optimisticDiscard) {
    return false;
  }

  if (
    hasLocalTurnKongPrompt ||
    canUseClaimMultiSelect(state) ||
    state.selectionMode === 'kong' ||
    state.selectionMode === 'chow' ||
    state.selectionMode === 'pung'
  ) {
    return false;
  }

  const localSeat = state.roomSnapshot?.payload.local_seat;
  if (typeof localSeat !== 'number') {
    return false;
  }

  const pendingAction = state.roomSnapshot?.payload.private_state?.pending_action;
  if (
    pendingAction?.type === 'active_turn' &&
    pendingAction.seat_index === localSeat &&
    Array.isArray(pendingAction.options) &&
    pendingAction.options.includes('discard')
  ) {
    return true;
  }

  return state.latestActionPrompt?.payload.seat_index === localSeat && state.latestActionPrompt.payload.options.includes('discard');
}

function isStandaloneBotSeat(seat: { seat_type?: string; is_bot?: boolean }) {
  if (seat.seat_type) {
    return seat.seat_type === 'bot';
  }

  return Boolean(seat.is_bot);
}

export function hasInviteableTableSeat(snapshot: SessionState['roomSnapshot']) {
  const payload = snapshot?.payload;
  if (!payload) {
    return false;
  }

  if (payload.seats.some(isStandaloneBotSeat)) {
    return true;
  }

  if (payload.phase === 'waiting' && payload.seats.length < TABLE_SEAT_CAPACITY) {
    return true;
  }

  return false;
}

export function createPendingWaitingRoomSnapshot(table: CreateTableResponse): RoomSnapshotMessage {
  return {
    type: 'room_snapshot',
    payload: {
      table_code: table.table_code,
      phase: table.phase,
      mode: table.mode,
      owner_user_id: table.owner_user_id,
      multiplier: table.multiplier,
      seats: table.seats,
      local_seat: 0,
      match_state: null,
      private_state: null,
    },
  } as const;
}

export function isActionBlockedByOptimisticDiscard(actionId: BattleActionId) {
  return (
    actionId === 'discard' ||
    actionId === 'ready_hand' ||
    actionId === 'flower' ||
    actionId === 'kong' ||
    actionId === 'hu' ||
    actionId === 'chow' ||
    actionId === 'pung' ||
    actionId === 'pass'
  );
}

export function upsertInvite(current: TableInvite[], nextInvite: TableInvite) {
  if (!isPendingTableInvite(nextInvite)) {
    return removeInviteById(current, nextInvite.id);
  }

  return [
    nextInvite,
    ...current.filter(
      (invite) => invite.id !== nextInvite.id && invite.inviter_user_id !== nextInvite.inviter_user_id,
    ),
  ];
}

export function removeInviteById(current: TableInvite[], inviteId: number) {
  return current.filter((invite) => invite.id !== inviteId);
}

function isPendingTableInvite(invite: TableInvite) {
  return invite.status === 'pending';
}

export function getPendingTableInvites(invites: TableInvite[]) {
  return invites
    .filter(isPendingTableInvite)
    .reduceRight<TableInvite[]>((current, invite) => upsertInvite(current, invite), []);
}

export function updateUserPoints(user: PublicUser | null, userId: number, points: number, title?: string) {
  if (!user || user.user_id !== userId) {
    return user;
  }

  const nextTitle = title ?? titleForPoints(points);
  return {
    ...user,
    points,
    title: nextTitle,
    display_label: `${user.display_name} | ${nextTitle}`,
  };
}

export function updateLeaderboardUserPoints(leaderboard: PublicUser[], userId: number, points: number, title?: string) {
  return leaderboard.map((user) => updateUserPoints(user, userId, points, title) ?? user);
}

export function updateUserActiveTableCode(
  user: PublicUser | null,
  userId: number,
  tableCode: string | null,
  tablePhase?: PublicUser['active_table_phase'],
) {
  if (!user || user.user_id !== userId) {
    return user;
  }

  return {
    ...user,
    active_table_code: tableCode,
    active_table_phase: tableCode ? tablePhase ?? user.active_table_phase ?? null : null,
  };
}

export function updateLeaderboardUserActiveTableCode(
  leaderboard: PublicUser[],
  userId: number,
  tableCode: string | null,
  tablePhase?: PublicUser['active_table_phase'],
) {
  return leaderboard.map((user) =>
    user.user_id === userId
      ? {
          ...user,
          active_table_code: tableCode,
          active_table_phase: tableCode ? tablePhase ?? user.active_table_phase ?? null : null,
        }
      : user,
  );
}

export function isStaleTableInviteError(error: unknown) {
  return error instanceof Error && error.message === 'table_not_found';
}

export function getUserDisplayName(users: PublicUser[], userId: number) {
  return users.find((user) => user.user_id === userId)?.display_name ?? `用户 #${userId}`;
}

export function getInviteCreatorLabel(invite: TableInvite, labelsByUserId: Record<number, string>) {
  return labelsByUserId[invite.inviter_user_id] ?? `用户 #${invite.inviter_user_id}`;
}

export function createInviteDialogUsers(users: PublicUser[], onlineUserIds: number[]): InviteDialogUser[] {
  const onlineUserIdSet = new Set(onlineUserIds);
  const activeTableUserCounts = users.reduce((counts, user) => {
    if (!user.active_table_code) {
      return counts;
    }

    counts.set(user.active_table_code, (counts.get(user.active_table_code) ?? 0) + 1);
    return counts;
  }, new Map<string, number>());

  return users.map((user) => ({
    user,
    status: getInviteDialogStatus(user, onlineUserIdSet, activeTableUserCounts),
  }));
}

function getInviteDialogStatus(
  user: PublicUser,
  onlineUserIdSet: Set<number>,
  activeTableUserCounts: Map<string, number>,
): PlayerInviteStatus {
  if (user.is_special_bot) {
    return user.active_table_code ? 'playing' : 'online';
  }

  if (!onlineUserIdSet.has(user.user_id)) {
    return 'offline';
  }

  if (!user.active_table_code) {
    return 'online';
  }

  return (activeTableUserCounts.get(user.active_table_code) ?? 0) > 1 ? 'playing' : 'online';
}
