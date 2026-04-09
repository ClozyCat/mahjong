export type Seat = 'bottom' | 'left' | 'top' | 'right';
export type TableMode = 'normal' | 'skill' | 'test';
export type SeatType = 'human' | 'bot';

export type MatchPhase =
  | 'loading'
  | 'watching'
  | 'my_turn'
  | 'resolving'
  | 'finished'
  | 'disconnected_or_waiting';

export type RoomPhase = 'waiting' | 'playing' | 'settlement' | 'finished';

export type ConnectionStatus = 'idle' | 'connecting' | 'connected' | 'reconnecting' | 'closed' | 'error';

export type BackendActionType = 'discard' | 'flower' | 'kong' | 'hu' | 'chow' | 'pung' | 'pass';
export type SkillActionType = 'select_skill' | 'decline_skill' | `skill:${string}`;
export type PromptActionType = BackendActionType | 'select_skill' | 'decline_skill';
export type ActionRequestType = BackendActionType | SkillActionType;
export type ClaimActionId = Extract<BackendActionType, 'kong' | 'chow' | 'pung'>;
export type QuickChatEmoji = string;
export type SkillRarity = 'common' | 'rare' | 'epic';
export type SkillInteractionKind = 'confirm' | 'preview_wall' | 'select_target' | 'select_hand_tile' | 'select_meld';

export interface HealthResponse {
  status: string;
}

export interface CreateTableResponse {
  table_code: string;
  phase: RoomPhase;
  mode?: TableMode;
  created_at: string;
  seats: SeatSnapshot[];
}

export interface SeatSnapshot {
  seat_index: number;
  nickname: string;
  connected: boolean;
  ready: boolean;
  is_bot?: boolean;
  seat_type?: SeatType;
}

export interface BackendMatchSeatStatistics {
  score_history: number[];
  win_count: number;
}

export interface BackendMatchStatistics {
  completed_round_count: number;
  seat_stats_by_seat: Record<string, BackendMatchSeatStatistics>;
}

export interface ConcealedTile {
  tile_id: string;
  tile_key: string;
}

export interface MatchState {
  prevailing_wind: 'east' | 'south' | 'west' | 'north';
  hand_number: number;
  dealer_seat: number;
  cumulative_scores: Record<string, number>;
  match_finished: boolean;
  last_completed_round_id: string | null;
  statistics?: BackendMatchStatistics | null;
}

export interface KongScoreDetail {
  kong_type: string;
  actor_seat: number;
  payer_seats: number[];
  delta_by_seat: Record<string, number>;
}

export interface ScoreState {
  flower_count_by_seat: Record<string, number>;
  kong_score_detail: KongScoreDetail[];
  kong_delta_by_seat: Record<string, number>;
  current_round_delta_by_seat: Record<string, number>;
  base_cumulative_scores: Record<string, number>;
  projected_cumulative_scores: Record<string, number>;
}

export interface PrivatePlayerState {
  seat_index: number;
  nickname: string;
  connected: boolean;
  concealed_count: number;
  concealed_tiles?: ConcealedTile[] | null;
  melds: string[][];
  flowers: string[];
  discards: string[];
  equipped_skill?: BackendSkillView | null;
}

export interface BackendSkillView {
  skill_id: string;
  serial?: string | null;
  name: string;
  rarity: SkillRarity;
  rarity_label: string;
  tone: 'jade' | 'azure' | 'violet';
  type: 'active' | 'passive';
  type_label: string;
  interaction_kind?: SkillInteractionKind | null;
  summary: string;
  detail: string;
  interaction_hint?: string | null;
  tags: string[];
  remaining_rounds: number;
  remaining_activations_this_round: number;
  can_activate_now?: boolean;
}

export interface BackendSkillSelectionView {
  cycle_key: string;
  cycle_label: string;
  deadline_at: string;
  title: string;
  detail: string;
  options: BackendSkillView[];
}

export interface BackendVisibleEffectView {
  effect_id: string;
  effect_type: string;
  owner: number;
  target_seats: number[];
  remaining_turns?: number | null;
  stacks: number;
  source_skill?: string | null;
  payload: Record<string, unknown>;
}

export interface BackendKnowledgeView {
  target_seat?: number | null;
  tile_ids: string[];
  tile_keys: string[];
  source_skill?: string | null;
  description?: string | null;
}

export type PendingAction =
  | {
      type: 'opening_flowers';
      seat_index: number;
      deadline_at: string;
      options: PromptActionType[];
    }
  | {
      type: 'active_turn';
      seat_index: number;
      deadline_at: string;
      drawn_tile_id?: string;
      restricted_discard_tile_ids?: string[];
      options: PromptActionType[];
    }
  | {
      type: 'claim_window';
      discarder_seat: number;
      deadline_at: string;
      responded_seats: number[];
      options: PromptActionType[];
    }
  | {
      type: 'rob_kong_window';
      actor_seat: number;
      tile_key: string;
      deadline_at: string;
      responded_seats: number[];
      options: PromptActionType[];
    }
  | {
      type: 'skill_draft';
      seat_index: number;
      deadline_at: string;
      options: PromptActionType[];
    }
  | Record<string, unknown>;

export interface PrivateState {
  round_id: string;
  round_wind: 'east' | 'south' | 'west' | 'north';
  dealer_seat: number;
  current_actor: number;
  wall_tiles_remaining?: number;
  last_discard?: string | null;
  pending_action?: PendingAction | null;
  skill_draft?: BackendSkillSelectionView | null;
  equipped_skills?: BackendSkillView[] | null;
  visible_effects?: BackendVisibleEffectView[] | null;
  private_knowledge?: BackendKnowledgeView[] | null;
  score_state?: ScoreState | null;
  players: PrivatePlayerState[];
}

export interface RoomSnapshotPayload {
  table_code: string;
  phase: RoomPhase;
  mode?: TableMode;
  seats: SeatSnapshot[];
  local_seat?: number | null;
  reconnect_token?: string | null;
  match_state?: MatchState | null;
  private_state?: PrivateState | null;
  continue_action?:
    | {
        action_id: Extract<BattleActionId, 'start_next_round' | 'restart_match'>;
        confirmed_seats: number[];
        required_seats: number[];
        online_seats: number[];
        auto_advance_deadline_at?: string | null;
      }
    | null;
}

export interface RoomSnapshotMessage {
  type: 'room_snapshot';
  payload: RoomSnapshotPayload;
}

export interface MatchResultPayload {
  table_code: string;
  round_id: string;
  phase: 'settlement';
  win_type: 'self_draw' | 'discard' | 'draw';
  display_win_label?: string | null;
  winner_seat?: number | null;
  discarder_seat?: number | null;
  fan_total: number;
  fan_keys: string[];
  fan_breakdown: Array<{
    fan_key: string;
    fan_value: number;
  }>;
  flower_count: number;
  kong_score_detail: KongScoreDetail[];
  score_delta: {
    provisional: boolean;
    basic_points?: number;
    base_points?: number;
    fan_total: number;
    minimum_qualifying_fan_total?: number;
    fan_delta_by_seat: Record<string, number>;
    kong_delta_by_seat: Record<string, number>;
    total_delta_by_seat: Record<string, number>;
  };
  draw_type?: string;
}

export interface MatchResultMessage {
  type: 'match_result';
  payload: MatchResultPayload;
}

export interface ActionPromptMessage {
  type: 'action_prompt';
  payload: {
    seat_index: number;
    options: PromptActionType[];
    deadline_at: string;
  };
}

export interface PlayerPresenceMessage {
  type: 'player_presence';
  payload: {
    table_code: string;
    seat_index: number;
    connected: boolean;
  };
}

export interface RoundEventMessage {
  type: 'round_event';
  payload: {
    event_type: string;
    event: Record<string, unknown>;
  };
}

export interface ActionRejectedMessage {
  type: 'action_rejected';
  payload: {
    reason: string;
  };
}

export interface HeartbeatMessage {
  type: 'heartbeat';
  payload: {
    sent_at?: string;
  };
}

export interface LeaveTableAcceptedMessage {
  type: 'leave_table_accepted';
  payload: {
    table_code: string;
    seat_index: number;
  };
}

export interface QuickChatMessage {
  type: 'quick_chat';
  payload: {
    message_id: string;
    actor_seat: number;
    target_seat: number;
    emoji: QuickChatEmoji;
    sent_at: string;
  };
}

export type ServerMessage =
  | RoomSnapshotMessage
  | MatchResultMessage
  | ActionPromptMessage
  | PlayerPresenceMessage
  | RoundEventMessage
  | ActionRejectedMessage
  | LeaveTableAcceptedMessage
  | QuickChatMessage
  | HeartbeatMessage;

export type ClientMessage =
  | { type: 'join_table'; payload: { nickname: string } }
  | { type: 'reconnect'; payload: { reconnect_token: string } }
  | { type: 'leave_table'; payload: Record<string, never> }
  | { type: 'ready'; payload: { ready: boolean } }
  | { type: 'adjust_bots'; payload: { delta: 1 | -1 } }
  | { type: 'start_match'; payload: Record<string, never> }
  | { type: 'start_next_round'; payload: Record<string, never> }
  | { type: 'restart_match'; payload: Record<string, never> }
  | { type: 'action_request'; payload: { action_type: ActionRequestType; tile_ids: string[] } }
  | { type: 'quick_chat'; payload: { target_seat: number; emoji: QuickChatEmoji } }
  | { type: 'heartbeat'; payload: { sent_at: string } };

export interface ToastMessage {
  id: string;
  kind: 'presence' | 'event' | 'error' | 'system';
  text: string;
  createdAt: string;
}

export interface MatchSeatStatisticsState {
  scoreHistory: number[];
  winCount: number;
}

export interface MatchStatisticsState {
  completedRoundCount: number;
  lastAppliedRoundId: string | null;
  seatStatsBySeat: Record<string, MatchSeatStatisticsState>;
}

export interface OptimisticDiscardState {
  tileId: string;
  tileCode: string;
  seatIndex: number;
  actionEffectKey: string;
  requestedAt: string;
}

export interface SessionState {
  apiBaseUrl?: string;
  wsBaseUrl?: string;
  tableCode: string;
  nickname: string;
  connectionStatus: ConnectionStatus;
  roomSnapshot: RoomSnapshotMessage | null;
  latestMatchResult: MatchResultMessage | null;
  latestActionPrompt: ActionPromptMessage | null;
  latestRoundEvent: RoundEventMessage | null;
  latestQuickChatMessage?: QuickChatMessage | null;
  lastRejectedAction: ActionRejectedMessage | null;
  reconnectToken: string | null;
  optimisticDiscard?: OptimisticDiscardState | null;
  selectedTileIds: string[];
  selectionMode: 'single' | 'kong' | 'chow' | 'pung' | null;
  toasts: ToastMessage[];
  matchStatistics?: MatchStatisticsState | null;
}

export type BattleActionId =
  | 'ready'
  | 'start_match'
  | 'start_next_round'
  | 'restart_match'
  | 'activate_skill'
  | BackendActionType;

export interface BattleActionView {
  id: BattleActionId;
  label: string;
  enabled: boolean;
  emphasis: 'high' | 'medium' | 'low';
}

export interface PlayerSkillView {
  skillId: string;
  serial?: string | null;
  name: string;
  rarity: SkillRarity;
  rarityLabel: string;
  tone: 'jade' | 'azure' | 'violet';
  type: 'active' | 'passive';
  typeLabel: string;
  summary: string;
  detail: string;
  interactionKind?: SkillInteractionKind | null;
  interactionHint?: string | null;
  tags: string[];
  cycleLabel?: string | null;
  remainingRounds: number;
  remainingActivationsThisRound: number;
  canActivateNow?: boolean;
  previewTileKeys?: string[];
}

export interface PlayerView {
  seat: Seat;
  absoluteSeat?: number;
  name: string;
  seatType?: SeatType;
  score: number;
  liveDelta: number;
  flowerCount: number;
  wind: 'East' | 'South' | 'West' | 'North';
  isDealer: boolean;
  isActive: boolean;
  isLocal: boolean;
  connected: boolean;
  isBotControlled?: boolean;
  ready: boolean;
  concealedCount: number;
  meldCount: number;
  melds: string[][];
  flowers: string[];
  statusText?: string;
  skill?: PlayerSkillView | null;
}

export interface WaitingControls {
  canReady: boolean;
  canStart: boolean;
  isReady: boolean;
  occupiedSeats: number;
  botCount: number;
  canAddBot: boolean;
  canRemoveBot: boolean;
}

export interface LocalTileView {
  tileId: string;
  code: string;
  isSelected: boolean;
  isDrawn: boolean;
  isFlower: boolean;
  isDisabled?: boolean;
}

export interface ClaimCandidateTileView {
  code: string;
  source: 'hand' | 'claim';
}

export interface ClaimCandidateView {
  key: string;
  actionId: ClaimActionId;
  actionLabel: string;
  tileIds: string[];
  tiles: ClaimCandidateTileView[];
  isSelected: boolean;
}

export interface ReadyHandWaitView {
  code: string;
  availableCount: number;
}

export interface ReadyHandInsightView {
  source: 'current' | 'selected_discard';
  discardTileId: string | null;
  discardTileCode: string | null;
  waits: ReadyHandWaitView[];
}

export interface ResultSeatView {
  seat: Seat;
  name: string;
  score: number;
  delta: number | null;
  stats?: {
    scoreHistory: number[];
    winCount: number;
    completedRoundCount: number;
    winRate: number;
  } | null;
}

export interface ResultView {
  title: string;
  summary: string;
  fanTotal: number | null;
  winnerSeat: Seat | null;
  discarderSeat: Seat | null;
  winType: string | null;
  winTypeLabel: string | null;
  provisional: boolean;
  flowerCount: number;
  fanBreakdown: Array<{
    fanKey: string;
    fanValue: number;
  }>;
  scoreDeltaBySeat: Partial<Record<Seat, number>>;
  seats: ResultSeatView[];
  continueAction:
    | {
        id: Extract<BattleActionId, 'start_next_round' | 'restart_match'>;
        label: string;
        enabled: boolean;
        confirmation?: {
          confirmedCount: number;
          requiredCount: number;
          isLocalConfirmed: boolean;
          countdownDeadlineAt?: string | null;
        };
        countdownDeadlineAt?: string | null;
      }
    | null;
}

export interface ActionEffectView {
  key: string;
  label: string;
  emphasis: 'draw' | 'discard' | 'claim' | 'kong' | 'system';
  seat: Seat | null;
  calloutTone?: 'chow' | 'pung' | 'kong' | 'hu' | 'skill' | null;
}

export interface BattlePromptView {
  kind: 'turn' | 'claim' | 'rob_kong' | 'turn_kong';
  tone: 'info' | 'urgent' | 'critical';
  title: string;
  detail: string | null;
  actionIds: BackendActionType[];
  highlightedActionIds: BackendActionType[];
  sourceSeat: Seat | null;
  isUrgent: boolean;
}

export interface QuickChatEventView {
  key: string;
  actorSeat: Seat;
  targetSeat: Seat;
  actorName: string;
  targetName: string;
  emoji: QuickChatEmoji;
  text: string;
}

export interface SkillChoiceView extends PlayerSkillView {
  cycleKey: string;
}

export interface SkillSelectionView {
  cycleKey: string;
  cycleLabel: string;
  deadlineAt: string;
  title: string;
  detail: string;
  options: SkillChoiceView[];
}

export interface SkillActivationChoiceView {
  id: string;
  label: string;
  description?: string;
  selected: boolean;
}

export interface SkillActivationTileChoiceView {
  tileId: string;
  code: string;
  label: string;
  selected: boolean;
}

export interface SkillActivationMeldChoiceView {
  index: number;
  label: string;
  tiles: string[];
  selected: boolean;
}

export interface SkillActivationPreviewTileView {
  key: string;
  code: string;
  label: string;
}

export interface SkillActivationView {
  skill: PlayerSkillView;
  kind: SkillInteractionKind;
  title: string;
  description: string;
  confirmLabel: string;
  canConfirm: boolean;
  targetChoices?: SkillActivationChoiceView[];
  handChoices?: SkillActivationTileChoiceView[];
  meldChoices?: SkillActivationMeldChoiceView[];
  previewTiles?: SkillActivationPreviewTileView[];
}

export interface BattleViewModel {
  roomMode: TableMode;
  mode: MatchPhase;
  tableCode: string;
  canLeaveTable: boolean;
  phaseLabel: string;
  roundLabel: string;
  scoreSummaryLabel: string;
  deadlineAt: string | null;
  topStatusLabel: string;
  activePlayerSeat: Seat;
  actionIndicatorSeat: Seat | null;
  isActionDockElevated: boolean;
  players: PlayerView[];
  actions: BattleActionView[];
  waitingControls: WaitingControls | null;
  discards: Record<Seat, string[]>;
  localHand: LocalTileView[];
  readyHandInsight: ReadyHandInsightView | null;
  claimCandidates: ClaimCandidateView[];
  drawnTileId: string | null;
  centerBanner: string | null;
  remainingTileCount?: number | null;
  promptText: string | null;
  promptCue: BattlePromptView | null;
  result: ResultView | null;
  settlementHands: Partial<Record<Seat, string[]>> | null;
  lastDiscard: string | null;
  lastDiscardSeat: Seat | null;
  shouldAutoReturnLastDiscardToRiver: boolean;
  actionEffect: ActionEffectView | null;
  quickChatEvent?: QuickChatEventView | null;
  skillSelection?: SkillSelectionView | null;
  skillActivation?: SkillActivationView | null;
  toasts: ToastMessage[];
}
