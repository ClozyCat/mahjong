// 协议类型,严格按 frontend/api.md 定义

export type RoomPhase = "waiting" | "playing" | "settlement" | "finished";
export type RoomMode = "normal" | "skill" | "test";
export type SeatIndex = 0 | 1 | 2 | 3;
export type Wind = "east" | "south" | "west" | "north";

export type PromptActionType =
  | "discard"
  | "flower"
  | "kong"
  | "hu"
  | "chow"
  | "pung"
  | "pass"
  | "select_skill"
  | "decline_skill"
  | (string & {});

export interface PublicSeatView {
  seat_index: number;
  nickname: string | null;
  connected: boolean;
  ready: boolean;
  is_bot: boolean;
  seat_type: "human" | "bot";
}

export interface SeatStatistics {
  score_history: number[];
  win_count: number;
}

export interface MatchState {
  prevailing_wind: Wind;
  hand_number: number;
  dealer_seat: number;
  cumulative_scores: Record<string, number>;
  match_finished: boolean;
  last_completed_round_id: string | null;
  statistics?: {
    completed_round_count: number;
    seat_stats_by_seat: Record<string, SeatStatistics>;
  };
}

export interface TileRef {
  tile_id: string;
  tile_key: string;
}

export interface PlayerSeatView {
  seat_index: number;
  nickname: string | null;
  connected: boolean;
  concealed_count: number;
  concealed_tiles: TileRef[] | null;
  melds: string[][];
  flowers: string[];
  discards: string[];
  equipped_skill: EquippedSkillView | null;
}

export interface OpeningFlowersPrompt {
  type: "opening_flowers";
  seat_index: number;
  deadline_at: string | null;
  options: PromptActionType[];
}

export interface ActiveTurnPrompt {
  type: "active_turn";
  seat_index: number;
  deadline_at: string | null;
  drawn_tile_id: string | null;
  restricted_discard_tile_ids: string[];
  options: PromptActionType[];
}

export interface ClaimWindowPrompt {
  type: "claim_window";
  discarder_seat: number;
  deadline_at: string | null;
  responded_seats: number[];
  options: PromptActionType[];
}

export interface RobKongWindowPrompt {
  type: "rob_kong_window";
  actor_seat: number;
  tile_key: string;
  deadline_at: string | null;
  responded_seats: number[];
  options: PromptActionType[];
}

export interface SkillDraftPrompt {
  type: "skill_draft";
  seat_index: number;
  deadline_at: string | null;
  options: PromptActionType[];
}

export type PendingAction =
  | OpeningFlowersPrompt
  | ActiveTurnPrompt
  | ClaimWindowPrompt
  | RobKongWindowPrompt
  | SkillDraftPrompt;

export interface SkillOption {
  skill_id: string;
  serial?: string;
  name: string;
  rarity: string;
  rarity_label: string;
  tone: string;
  type: "active" | "passive" | (string & {});
  type_label: string;
  interaction_kind: string;
  summary: string;
  detail: string;
  interaction_hint: string;
  tags: string[];
  remaining_rounds: number;
  remaining_activations_this_round: number;
}

export interface SkillDraftState {
  cycle_key: string;
  cycle_label: string;
  deadline_at: string | null;
  title: string;
  detail: string;
  options: SkillOption[];
}

export interface EquippedSkillView extends SkillOption {
  can_activate_now?: boolean;
}

export interface VisibleEffect {
  effect_id: string;
  effect_type: string;
  owner: number;
  target_seats: number[];
  remaining_turns: number;
  stacks: number;
  source_skill: string;
  payload: Record<string, unknown>;
}

export interface PrivateKnowledge {
  target_seat: number;
  tile_ids: string[];
  tile_keys: string[];
  source_skill: string;
  description: string;
}

export interface ScoreState {
  flower_count_by_seat: Record<string, number>;
  kong_score_detail: unknown[];
  kong_delta_by_seat: Record<string, number>;
  current_round_delta_by_seat: Record<string, number>;
  base_cumulative_scores: Record<string, number>;
  projected_cumulative_scores: Record<string, number>;
}

export interface PrivateState {
  round_id: string;
  round_wind: Wind;
  dealer_seat: number;
  current_actor: number;
  wall_tiles_remaining: number;
  last_discard: string | null;
  pending_action: PendingAction | null;
  skill_draft: SkillDraftState | null;
  score_state: ScoreState;
  equipped_skills: EquippedSkillView[];
  visible_effects: VisibleEffect[];
  private_knowledge: PrivateKnowledge[];
  players: PlayerSeatView[];
}

export interface ContinueActionView {
  action_id: "start_next_round" | "restart_match" | (string & {});
  confirmed_seats: number[];
  required_seats: number[];
  online_seats: number[];
  auto_advance_deadline_at: string | null;
}

export interface RoomSnapshot {
  table_code: string;
  phase: RoomPhase;
  mode: RoomMode;
  seats: PublicSeatView[];
  local_seat: number;
  reconnect_token: string | null;
  match_state: MatchState | null;
  private_state: PrivateState | null;
  continue_action: ContinueActionView | null;
}

// ============ Round events ============

export interface RoundEventTileDiscarded {
  type: "tile_discarded";
  seat: number;
  tile_id: string;
  tile_key: string;
}
export interface RoundEventFlowerExposed {
  type: "flower_exposed";
  seat: number;
  tile_id: string;
}
export interface RoundEventReplacementDraw {
  type: "replacement_draw";
  seat: number;
  tile_id: string;
  tile_key: string;
}
export interface RoundEventClaimMade {
  type: "claim_made";
  seat: number;
  from: number;
  claim_type: "chow" | "pung" | "kong" | "hu" | (string & {});
  tile_id: string;
  tile_key: string;
  meld?: string[];
}
export interface RoundEventSelfHuDeclared {
  type: "self_hu_declared";
  seat: number;
  tile_id: string;
}
export interface RoundEventSelfKongDeclared {
  type: "self_kong_declared";
  seat: number;
  kong_type: "concealed_kong" | "add_kong" | (string & {});
  tile_key: string;
  tile_ids: string[];
}
export interface RoundEventClaimAutoPassed {
  type: "claim_auto_passed";
  discarder_seat: number;
  seats: number[];
}
export interface RoundEventRobKongAutoPassed {
  type: "rob_kong_auto_passed";
  actor_seat: number;
  seats: number[];
}
export interface RoundEventSettlementReady {
  type: "settlement_ready";
  round_id: string;
  settlement: Record<string, unknown>;
}
export interface RoundEventRoundDrawn {
  type: "round_drawn";
  round_id: string;
  settlement: Record<string, unknown>;
}
export interface RoundEventSkillActivated {
  type: "skill_activated";
  seat: number;
  skill_id: string;
}
export interface RoundEventSkillTileReplaced {
  type: "skill_tile_replaced";
  seat: number;
  removed_tile_id: string;
  replacement_tile_id: string;
  replacement_tile_key: string;
}
export interface RoundEventSkillReclaimMeld {
  type: "skill_reclaim_meld";
  seat: number;
  meld_index: number;
  tile_keys: string[];
}
export interface RoundEventSkillForceDraw {
  type: "skill_force_draw";
  seat: number;
  penalty: number;
  next_round_penalty: number;
}
export interface RoundEventSkillScoreAdjusted {
  type: "skill_score_adjusted";
  seat: number;
  delta: number;
  reason: string;
}

export type RoundEvent =
  | RoundEventTileDiscarded
  | RoundEventFlowerExposed
  | RoundEventReplacementDraw
  | RoundEventClaimMade
  | RoundEventSelfHuDeclared
  | RoundEventSelfKongDeclared
  | RoundEventClaimAutoPassed
  | RoundEventRobKongAutoPassed
  | RoundEventSettlementReady
  | RoundEventRoundDrawn
  | RoundEventSkillActivated
  | RoundEventSkillTileReplaced
  | RoundEventSkillReclaimMeld
  | RoundEventSkillForceDraw
  | RoundEventSkillScoreAdjusted
  | { type: string; [k: string]: unknown };

// ============ Server -> Client ============

export interface ActionPromptPayload {
  seat_index: number;
  options: PromptActionType[];
  deadline_at: string | null;
}

export interface MatchResultPayload {
  table_code: string;
  round_id: string;
  phase: RoomPhase;
  provisional: boolean;
  win_type: "discard" | "self_draw" | "draw" | (string & {});
  winner_seat: number | null;
  discarder_seat: number | null;
  display_win_label: string | null;
  fan_total: number;
  fan_keys: string[];
  fan_breakdown: { fan_key: string; fan_value: number }[];
  score_delta: {
    provisional: boolean;
    basic_points: number;
    base_points: number;
    fan_total: number;
    minimum_qualifying_fan_total: number;
    fan_delta_by_seat: Record<string, number>;
    kong_delta_by_seat: Record<string, number>;
    total_delta_by_seat: Record<string, number>;
  };
  flower_count: number;
  draw_type: "exhaustive" | "skill_forced" | null | (string & {});
  kong_score_detail: unknown[];
}

export interface PlayerPresencePayload {
  table_code: string;
  seat_index: number;
  connected: boolean;
}

export interface QuickChatPayload {
  message_id: string;
  actor_seat: number;
  target_seat: number;
  emoji: string;
  sent_at: string;
}

export interface LeaveTableAcceptedPayload {
  table_code: string;
  seat_index: number;
}

export interface ActionRejectedPayload {
  reason: string;
}

export type ServerMessage =
  | { type: "room_snapshot"; payload: RoomSnapshot }
  | {
      type: "round_event";
      payload: { event_type: string; event: RoundEvent };
    }
  | { type: "action_prompt"; payload: ActionPromptPayload }
  | { type: "match_result"; payload: MatchResultPayload }
  | { type: "player_presence"; payload: PlayerPresencePayload }
  | { type: "quick_chat"; payload: QuickChatPayload }
  | { type: "leave_table_accepted"; payload: LeaveTableAcceptedPayload }
  | { type: "action_rejected"; payload: ActionRejectedPayload }
  | { type: "heartbeat"; payload: { request_id?: string; sent_at?: string } };

// ============ Client -> Server ============

export type ClientMessage =
  | { type: "join_table"; payload: { nickname: string } }
  | { type: "reconnect"; payload: { reconnect_token: string } }
  | { type: "ready"; payload: { ready: boolean } }
  | { type: "adjust_bots"; payload: { delta: 1 | -1 } }
  | { type: "start_match"; payload: Record<string, never> }
  | { type: "start_next_round"; payload: Record<string, never> }
  | { type: "restart_match"; payload: Record<string, never> }
  | { type: "leave_table"; payload: Record<string, never> }
  | {
      type: "quick_chat";
      payload: { target_seat: number; emoji: string };
    }
  | { type: "heartbeat"; payload: { sent_at: string } }
  | {
      type: "action_request";
      payload: { action_type: string; tile_ids: string[] };
    };

// ============ HTTP ============

export interface CreateTableRequest {
  table_code?: string;
  mode?: RoomMode;
  enforce_minimum_eight_fan?: boolean;
}

export interface CreateTableResponse {
  table_code: string;
  phase: RoomPhase;
  mode: RoomMode;
  created_at: string;
  seats: PublicSeatView[];
}
