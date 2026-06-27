export type Seat = 'bottom' | 'left' | 'top' | 'right';
export type TableMode = 'normal' | 'evaluation';
export type SeatType = 'human' | 'bot' | 'special_bot';
export type TableMultiplier = 1;
export type MinimumHuFan = 0 | 2 | 4 | 6 | 8;

export type MatchPhase =
  | 'loading'
  | 'watching'
  | 'my_turn'
  | 'resolving'
  | 'finished'
  | 'disconnected_or_waiting';

export type RoomPhase = 'waiting' | 'playing' | 'settlement' | 'finished';

export type ConnectionStatus = 'idle' | 'connecting' | 'connected' | 'reconnecting' | 'closed' | 'error';

export type BackendActionType =
  | 'discard'
  | 'ready_hand'
  | 'flower'
  | 'kong'
  | 'hu'
  | 'chow'
  | 'pung'
  | 'pass'
  | 'multiplier_1'
  | 'multiplier_2'
  | 'multiplier_3';
export type PromptActionType = BackendActionType;
export type ActionRequestType = BackendActionType;
export type ClaimActionId = Extract<BackendActionType, 'kong' | 'chow' | 'pung'>;
export type QuickChatEmoji = string;

export interface HealthResponse {
  status: string;
}

export interface PublicUser {
  user_id: number;
  username: string;
  display_name: string;
  points: number;
  title: string;
  display_label: string;
  bio: string;
  avatar?: string | null;
  active_table_code?: string | null;
  active_table_phase?: RoomPhase | null;
  is_special_bot?: boolean;
}

export interface UserBrief {
  user_id: number;
  display_name: string;
  points: number;
  title: string;
  display_label: string;
}

export interface AuthResponse {
  session_token: string;
  user: PublicUser;
}

export interface CreateTableResponse {
  table_code: string;
  phase: RoomPhase;
  mode?: TableMode;
  owner_user_id?: number | null;
  multiplier?: TableMultiplier;
  created_at: string;
  seats: SeatSnapshot[];
}

export interface ActiveTableResponse {
  table_code: string;
  seat_index: number;
  role: string;
}

export interface TableInvite {
  id: number;
  table_code: string;
  inviter_user_id: number;
  invitee_user_id: number;
  status: string;
  created_at: string;
  expires_at: string;
  accepted_at?: string | null;
}

export interface AcceptInviteResponse {
  invite_id: number;
  table_code: string;
  seat_index: number;
  status: string;
}

export interface EvaluationSubjectResult {
  subject_id: string;
  user_id?: number | null;
  display_name: string;
  kind: 'human' | 'bot' | string;
  table_code: string;
  phase: RoomPhase;
  completed: boolean;
  final_score?: number | null;
  deal_in_count?: number | null;
  win_count?: number | null;
  completed_round_count?: number | null;
}

export interface EvaluationSessionResponse {
  evaluation_id: string;
  seed: number;
  subjects: EvaluationSubjectResult[];
}

export interface SeatSnapshot {
  seat_index: number;
  user_id?: number | null;
  nickname: string;
  points?: number | null;
  title?: string | null;
  connected: boolean;
  is_bot?: boolean;
  seat_type?: SeatType;
}

export interface BackendMatchSeatStatistics {
  score_history: number[];
  win_count: number;
  deal_in_count: number;
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
  seed?: number;
  prevailing_wind: 'east' | 'south' | 'west' | 'north';
  hand_number: number;
  dealer_seat: number;
  dealer_repeat_count?: number;
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
  points?: number | null;
  title?: string | null;
  connected: boolean;
  is_ready_hand?: boolean;
  concealed_count: number;
  concealed_tiles?: ConcealedTile[] | null;
  melds: string[][];
  display_melds?: DisplayMeldView[];
  flowers: string[];
  discards: string[];
  selected_multiplier?: number;
}

export type PendingAction =
  | {
      type: 'active_turn';
      seat_index: number;
      deadline_at: string;
      drawn_tile_id?: string;
      restricted_discard_tile_ids?: string[];
      options: PromptActionType[];
      remaining_extra_time?: number;
      extended_with_extra?: boolean;
    }
  | {
      type: 'claim_window';
      discarder_seat: number;
      deadline_at: string;
      responded_seats: number[];
      options: PromptActionType[];
      remaining_extra_time?: number;
      extended_with_extra?: boolean;
    }
  | {
      type: 'rob_kong_window';
      actor_seat: number;
      tile_key: string;
      deadline_at: string;
      responded_seats: number[];
      options: PromptActionType[];
      remaining_extra_time?: number;
      extended_with_extra?: boolean;
    }
  | {
      type: 'player_multiplier_selection';
      deadline_at: string;
      responded_seats: number[];
      selected_multipliers: Record<string, number>;
      options: PromptActionType[];
      extended_with_extra?: boolean;
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
  hand_insights?: BackendHandInsights | null;
  score_state?: ScoreState | null;
  players: PrivatePlayerState[];
}

export interface RoomSnapshotPayload {
  table_code: string;
  server_now?: string;
  phase: RoomPhase;
  mode?: TableMode;
  owner_user_id?: number | null;
  multiplier?: TableMultiplier;
  minimum_hu_fan?: MinimumHuFan;
  dealer_repeat_enabled?: boolean;
  dealer_double_enabled?: boolean;
  player_multiplier_selection_enabled?: boolean;
  ready_hand_enabled?: boolean;
  seats: SeatSnapshot[];
  local_seat?: number | null;
  match_state?: MatchState | null;
  private_state?: PrivateState | null;
  continue_action?:
    | {
        action_id: Extract<BattleActionId, 'start_next_round'>;
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
  winning_details?: Array<{
    winner_seat: number;
    display_win_label?: string | null;
    fan_total: number;
    fan_keys: string[];
    fan_breakdown: Array<{
      fan_key: string;
      fan_value: number;
    }>;
    flower_count: number;
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
  settlement_seats?: SeatSnapshot[];
}

export interface MatchResultMessage {
  type: 'match_result';
  payload: MatchResultPayload;
}

export interface ActionPromptMessage {
  type: 'action_prompt';
  payload: {
    server_now?: string;
    seat_index: number;
    options: PromptActionType[];
    deadline_at: string;
    remaining_extra_time?: number;
  };
}

export interface PlayerPresenceMessage {
  type: 'player_presence';
  payload: {
    table_code: string;
    seat_index: number;
    connected: boolean;
    user_id?: number | null;
    nickname?: string | null;
    points?: number | null;
    title?: string | null;
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
    server_now?: string;
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
    chat_kind?: 'emoji' | 'point_gesture' | string;
    actor_kind?: 'player' | string;
    actor_display_name?: string | null;
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

export interface UserPresenceUpdatedMessage {
  type: 'user_presence_updated';
  payload: {
    online_user_ids: number[];
  };
}

export interface UserPointsUpdatedMessage {
  type: 'user_points_updated';
  payload: {
    user_id: number;
    delta: number;
    old_points?: number;
    points: number;
    old_title?: string;
    title?: string;
    display_name?: string;
    reason: 'round_settlement' | string;
    source_table_code?: string | null;
    source_round_id?: string | null;
  };
}

export interface UserActiveTableUpdatedMessage {
  type: 'user_active_table_updated';
  payload: {
    user_id: number;
    active_table_code: string | null;
    active_table_phase?: RoomPhase | null;
  };
}

export interface TableInviteCreatedMessage {
  type: 'table_invite_created';
  payload: TableInvite;
}

export interface TableInviteDecidedMessage {
  type: 'table_invite_decided';
  payload: TableInvite;
}

export type SocialServerMessage =
  | UserPresenceUpdatedMessage
  | UserPointsUpdatedMessage
  | UserActiveTableUpdatedMessage
  | TableInviteCreatedMessage
  | TableInviteDecidedMessage;

export type ClientMessage =
  | { type: 'join_table'; payload: { session_token: string } }
  | { type: 'leave_table'; payload: Record<string, never> }
  | { type: 'adjust_bots'; payload: { delta: 1 | -1 } }
  | { type: 'set_minimum_hu_fan'; payload: { minimum_hu_fan: MinimumHuFan } }
  | { type: 'set_dealer_repeat'; payload: { enabled: boolean } }
  | { type: 'set_dealer_double'; payload: { enabled: boolean } }
  | { type: 'set_player_multiplier_selection'; payload: { enabled: boolean } }
  | { type: 'set_bot_takeover'; payload: { enabled: boolean } }
  | { type: 'start_match'; payload: Record<string, never> }
  | { type: 'start_next_round'; payload: Record<string, never> }
  | { type: 'action_request'; payload: { action_type: ActionRequestType; tile_ids: string[] } }
  | { type: 'quick_chat'; payload: { target_seat: number; emoji: QuickChatEmoji; chat_kind?: 'emoji' | 'point_gesture' } }
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
  dealInCount: number;
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
  actionType: 'discard' | 'ready_hand';
  actionEffectKey: string;
  requestedAt: string;
}

export interface OptimisticFlowerState {
  tileId: string;
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
  recentRoundEvents?: RoundEventMessage[];
  latestReplacementTileId?: string | null;
  latestQuickChatMessage?: QuickChatMessage | null;
  latestSystemBroadcast?: SystemBroadcastEventView | null;
  lastRejectedAction: ActionRejectedMessage | null;
  optimisticDiscard?: OptimisticDiscardState | null;
  optimisticFlower?: OptimisticFlowerState | null;
  selectedTileIds: string[];
  selectionMode: 'single' | 'kong' | 'chow' | 'pung' | null;
  toasts: ToastMessage[];
  matchStatistics?: MatchStatisticsState | null;
  serverNowOffsetMs?: number;
}

export type BattleActionId =
  | 'invite'
  | 'start_match'
  | 'start_next_round'
  | 'match_decided'
  | BackendActionType;

export interface BattleActionView {
  id: BattleActionId;
  label: string;
  enabled: boolean;
  emphasis: 'high' | 'medium' | 'low';
}

export interface PlayerView {
  seat: Seat;
  absoluteSeat?: number;
  userId?: number | null;
  name: string;
  title?: string | null;
  seatType?: SeatType;
  score: number;
  points: number;
  liveDelta: number;
  flowerCount: number;
  wind: 'East' | 'South' | 'West' | 'North';
  isDealer: boolean;
  isActive: boolean;
  isLocal: boolean;
  connected: boolean;
  isBotControlled?: boolean;
  isReadyHand: boolean;
  selectedMultiplier?: number;
  showSelectedMultiplier?: boolean;
  concealedCount: number;
  meldCount: number;
  melds: PlayerMeldView[];
  flowers: string[];
  statusText?: string;
}

export interface DisplayMeldTileView {
  code: string;
  orientation: 'normal' | 'rotated' | 'upside_down' | 'face_down';
}

export interface DisplayMeldView {
  tiles: DisplayMeldTileView[];
}

export type PlayerMeldView = string[] | DisplayMeldView;

export interface WaitingControls {
  canStart: boolean;
  occupiedSeats: number;
  botCount: number;
  canAddBot: boolean;
  canRemoveBot: boolean;
  minimumHuFan: MinimumHuFan;
  canDecreaseMinimumHuFan: boolean;
  canIncreaseMinimumHuFan: boolean;
  dealerRepeatEnabled: boolean;
  dealerDoubleEnabled: boolean;
  playerMultiplierSelectionEnabled?: boolean;
  canToggleDealerRepeat: boolean;
  canToggleDealerDouble: boolean;
  canTogglePlayerMultiplierSelection?: boolean;
}

export interface TableSettingsView {
  minimumHuFan: MinimumHuFan;
  dealerRepeatEnabled: boolean;
  dealerDoubleEnabled: boolean;
  playerMultiplierSelectionEnabled?: boolean;
}

export interface LocalTileView {
  tileId: string;
  code: string;
  isSelected: boolean;
  isDrawn: boolean;
  isReplacementDrawn?: boolean;
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

export interface BackendHandInsightWait {
  code: string;
  available_count: number;
}

export interface BackendHandInsightWinningFan {
  fan_key: string;
  fan_value: number;
}

export interface BackendHandInsight {
  discard_tile_id: string | null;
  discard_tile_code: string | null;
  is_tenpai: boolean;
  waits: BackendHandInsightWait[];
  winning_fans: BackendHandInsightWinningFan[];
}

export interface BackendHandInsights {
  current: BackendHandInsight | null;
  by_discard_tile_id: Record<string, BackendHandInsight>;
}

export interface HandInsightWaitView {
  code: string;
  availableCount: number;
}

export interface HandInsightWinningFanView {
  fanKey: string;
  fanValue: number;
}

export interface HandInsightView {
  source: 'current' | 'selected_discard';
  discardTileId: string | null;
  discardTileCode: string | null;
  isTenpai: boolean;
  waits: HandInsightWaitView[];
  winningFans: HandInsightWinningFanView[];
}

export interface ResultSeatView {
  seat: Seat;
  absoluteSeat?: number;
  wind?: string;
  name: string;
  title?: string | null;
  displayLabel?: string;
  score: number;
  delta: number | null;
  stats?: {
    scoreHistory: number[];
    winCount: number;
    dealInCount: number;
    completedRoundCount: number;
    winRate: number;
  } | null;
}

export interface ResultPageView {
  fanTotal: number | null;
  winnerSeat: Seat | null;
  winnerAbsoluteSeat?: number | null;
  discarderSeat: Seat | null;
  discarderAbsoluteSeat?: number | null;
  winType: string | null;
  winTypeLabel: string | null;
  flowerCount: number;
  fanBreakdown: Array<{
    fanKey: string;
    fanValue: number;
  }>;
}

export interface ResultView {
  roundId?: string | null;
  title: string;
  summary: string;
  fanTotal: number | null;
  winnerSeat: Seat | null;
  winnerAbsoluteSeat?: number | null;
  discarderSeat: Seat | null;
  discarderAbsoluteSeat?: number | null;
  winType: string | null;
  winTypeLabel: string | null;
  provisional: boolean;
  flowerCount: number;
  fanBreakdown: Array<{
    fanKey: string;
    fanValue: number;
  }>;
  pages?: ResultPageView[];
  scoreDeltaBySeat: Partial<Record<Seat, number>>;
  seats: ResultSeatView[];
  continueAction:
    | {
        id: Extract<BattleActionId, 'start_next_round' | 'match_decided'>;
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
  calloutTone?: 'chow' | 'pung' | 'kong' | 'hu' | 'ready_hand' | 'multiplier' | null;
  tileCode?: string | null;
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

export interface SystemBroadcastEventView {
  key: string;
  text: string;
}

export interface DealerSelectionView {
  key: string;
  dealerSeat: Seat;
  dealerName: string;
  startedAt: string;
  revealAt: string;
  durationMs: number;
}

export interface BattleViewModel {
  roomMode: TableMode;
  mode: MatchPhase;
  tableCode: string;
  canLeaveTable: boolean;
  phaseLabel: string;
  roundLabel: string;
  deadlineAt: string | null;
  serverNowOffsetMs?: number;
  extendedWithExtra: boolean;
  activePlayerSeat: Seat;
  actionIndicatorSeat: Seat | null;
  shouldDebounceCenterWaiting?: boolean;
  isActionDockElevated: boolean;
  players: PlayerView[];
  actions: BattleActionView[];
  waitingControls: WaitingControls | null;
  tableSettings: TableSettingsView;
  discards: Record<Seat, string[]>;
  selectedTileCode?: string | null;
  localHand: LocalTileView[];
  handInsight: HandInsightView | null;
  claimCandidates: ClaimCandidateView[];
  drawnTileId: string | null;
  centerStatusText: string | null;
  remainingTileCount?: number | null;
  promptText: string | null;
  promptCue: BattlePromptView | null;
  result: ResultView | null;
  settlementHands: Partial<Record<Seat, string[]>> | null;
  lastDiscard: string | null;
  lastDiscardSeat: Seat | null;
  shouldAutoReturnLastDiscardToRiver: boolean;
  actionEffect: ActionEffectView | null;
  actionEffects?: ActionEffectView[];
  dealerSelection: DealerSelectionView | null;
  quickChatEvent?: QuickChatEventView | null;
  systemBroadcastEvent?: SystemBroadcastEventView | null;
}
