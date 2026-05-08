export type Seat = 'bottom' | 'left' | 'top' | 'right';
export type TableMode = 'normal';
export type SeatType = 'human' | 'bot';
export type ClientMode = 'player' | 'spectator';
export type TableMultiplier = 1;

export type MatchPhase =
  | 'loading'
  | 'watching'
  | 'my_turn'
  | 'resolving'
  | 'finished'
  | 'disconnected_or_waiting';

export type RoomPhase = 'waiting' | 'playing' | 'settlement' | 'finished';

export type ConnectionStatus = 'idle' | 'connecting' | 'connected' | 'reconnecting' | 'closed' | 'error';

export type BackendActionType = 'discard' | 'ready_hand' | 'flower' | 'kong' | 'hu' | 'chow' | 'pung' | 'pass';
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

export interface SpectatorRequest {
  id: number;
  table_code: string;
  requester_user_id: number;
  owner_user_id: number;
  status: string;
  created_at: string;
  decided_at?: string | null;
}

export interface SpectatorSnapshot {
  user_id: number;
  display_name: string;
}

export interface GameSummary {
  game_id: number;
  table_code: string;
  owner: UserBrief;
  multiplier: number;
  started_at: string;
  ended_at?: string | null;
  round_count: number;
  opponent_names: string[];
  player_summary?: UserGamePlayerSummary | null;
}

export interface UserGamePlayerSummary {
  round_count: number;
  win_count: number;
  self_draw_win_count: number;
  discard_win_count: number;
  deal_in_count: number;
  total_score_delta: number;
  average_cumulative_score: number;
  high_score_round_count: number;
}

export interface UserFanStat {
  user_id: number;
  fan_key: string;
  fan_label: string;
  count: number;
  last_seen_at: string;
}

export interface SeatSnapshot {
  seat_index: number;
  user_id?: number | null;
  nickname: string;
  points?: number | null;
  title?: string | null;
  connected: boolean;
  ready: boolean;
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
}

export type PendingAction =
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
  phase: RoomPhase;
  mode?: TableMode;
  owner_user_id?: number | null;
  multiplier?: TableMultiplier;
  seats: SeatSnapshot[];
  spectators?: SpectatorSnapshot[];
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

export interface SpectatorRequestCreatedMessage {
  type: 'spectator_request_created';
  payload: SpectatorRequest;
}

export interface SpectatorRequestDecidedMessage {
  type: 'spectator_request_decided';
  payload: SpectatorRequest;
}

export type SocialServerMessage =
  | UserPresenceUpdatedMessage
  | UserPointsUpdatedMessage
  | UserActiveTableUpdatedMessage
  | TableInviteCreatedMessage
  | TableInviteDecidedMessage
  | SpectatorRequestCreatedMessage
  | SpectatorRequestDecidedMessage;

export type ClientMessage =
  | { type: 'join_table'; payload: { session_token: string } }
  | { type: 'watch_table'; payload: { session_token: string; nickname?: string } }
  | { type: 'reconnect'; payload: { reconnect_token: string } }
  | { type: 'leave_table'; payload: Record<string, never> }
  | { type: 'ready'; payload: { ready: boolean } }
  | { type: 'adjust_bots'; payload: { delta: 1 | -1 } }
  | { type: 'set_bot_takeover'; payload: { enabled: boolean } }
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
  clientMode?: ClientMode;
  spectatorFocusSeat?: number | null;
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
  reconnectToken: string | null;
  optimisticDiscard?: OptimisticDiscardState | null;
  optimisticFlower?: OptimisticFlowerState | null;
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
  isReadyHand: boolean;
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
  name: string;
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
  discarderSeat: Seat | null;
  winType: string | null;
  winTypeLabel: string | null;
  flowerCount: number;
  fanBreakdown: Array<{
    fanKey: string;
    fanValue: number;
  }>;
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
  pages?: ResultPageView[];
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
  calloutTone?: 'chow' | 'pung' | 'kong' | 'hu' | 'ready_hand' | null;
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
  selectedTileCode?: string | null;
  localHand: LocalTileView[];
  handInsight: HandInsightView | null;
  claimCandidates: ClaimCandidateView[];
  drawnTileId: string | null;
  centerBanner: string | null;
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
  toasts: ToastMessage[];
}
