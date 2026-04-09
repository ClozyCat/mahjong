import type {
  BackendSkillView,
  BackendKnowledgeView,
  BattleActionView,
  BattleViewModel,
  PlayerSkillView,
  SessionState,
  SkillActivationMeldChoiceView,
  SkillActivationTileChoiceView,
  SkillActivationView,
  SkillInteractionKind,
} from '../types/match';

const SKILL_TOOLTIP_DELAY_MS = 500;

export const PLAYER_SKILL_TOOLTIP_DELAY_MS = SKILL_TOOLTIP_DELAY_MS;

interface SkillActivationState {
  skillId: string;
  interactionKind: SkillInteractionKind;
  selectedTargetSeat: number | null;
  selectedTileId: string | null;
  selectedMeldIndex: number | null;
}

export interface SkillRuntimeState {
  activation: SkillActivationState | null;
}

export function createInitialSkillRuntimeState(): SkillRuntimeState {
  return {
    activation: null,
  };
}

function isSkillMode(sessionState: SessionState) {
  return sessionState.roomSnapshot?.payload.mode === 'skill';
}

function toPlayerSkillView(skill: BackendSkillView | null | undefined): PlayerSkillView | null {
  if (!skill) {
    return null;
  }

  return {
    skillId: skill.skill_id,
    serial: skill.serial ?? null,
    name: skill.name,
    rarity: skill.rarity,
    rarityLabel: skill.rarity_label,
    tone: skill.tone,
    type: skill.type,
    typeLabel: skill.type_label,
    summary: skill.summary,
    detail: skill.detail,
    interactionKind: skill.interaction_kind ?? null,
    interactionHint: skill.interaction_hint ?? null,
    tags: Array.isArray(skill.tags) ? skill.tags : [],
    cycleLabel: null,
    remainingRounds: skill.remaining_rounds,
    remainingActivationsThisRound: skill.remaining_activations_this_round,
    canActivateNow: Boolean(skill.can_activate_now),
  };
}

function getLocalEquippedSkills(sessionState: SessionState): BackendSkillView[] {
  if (!isSkillMode(sessionState)) {
    return [];
  }
  return sessionState.roomSnapshot?.payload.private_state?.equipped_skills ?? [];
}

function getLocalPrivateKnowledge(sessionState: SessionState): BackendKnowledgeView[] {
  if (!isSkillMode(sessionState)) {
    return [];
  }

  return sessionState.roomSnapshot?.payload.private_state?.private_knowledge ?? [];
}

function buildPreviewTiles(
  sessionState: SessionState,
  skillId: string,
): SkillActivationView['previewTiles'] {
  const knowledgeEntries = getLocalPrivateKnowledge(sessionState).filter(
    (entry) => entry.source_skill === skillId && Array.isArray(entry.tile_keys),
  );

  return knowledgeEntries.flatMap((entry, knowledgeIndex) =>
    entry.tile_keys
      .filter((tileKey): tileKey is string => typeof tileKey === 'string' && tileKey.length > 0)
      .map((tileKey, tileIndex) => ({
        key: `${skillId}-${knowledgeIndex}-${tileIndex}-${tileKey}`,
        code: tileKey,
        label: `尾${tileIndex + 1}`,
      })),
  );
}

function getLocalActivatableSkill(sessionState: SessionState): PlayerSkillView | null {
  const skill = getLocalEquippedSkills(sessionState).find(
    (candidate) => candidate.type === 'active' && candidate.can_activate_now,
  );
  return toPlayerSkillView(skill);
}

function isActivationStillValid(runtime: SkillRuntimeState, sessionState: SessionState) {
  const activation = runtime.activation;
  if (!activation) {
    return true;
  }

  const skill = getLocalEquippedSkills(sessionState).find(
    (candidate) =>
      candidate.skill_id === activation.skillId &&
      candidate.type === 'active' &&
      candidate.can_activate_now &&
      candidate.interaction_kind === activation.interactionKind,
  );

  return Boolean(skill);
}

export function syncSkillRuntimeWithSession(runtime: SkillRuntimeState, sessionState: SessionState): SkillRuntimeState {
  if (!isSkillMode(sessionState)) {
    return {
      activation: null,
    };
  }

  if (sessionState.roomSnapshot?.payload.private_state?.skill_draft) {
    return {
      activation: null,
    };
  }

  if (isActivationStillValid(runtime, sessionState)) {
    return runtime;
  }

  return {
    activation: null,
  };
}

export function declineCurrentSkillOffer(runtime: SkillRuntimeState, _sessionState: SessionState): SkillRuntimeState {
  return runtime;
}

export function selectSkillForCurrentCycle(
  runtime: SkillRuntimeState,
  _sessionState: SessionState,
  _skillId: string,
): SkillRuntimeState {
  return runtime;
}

export function openSkillActivation(runtime: SkillRuntimeState, sessionState: SessionState): SkillRuntimeState {
  const skill = getLocalActivatableSkill(sessionState);
  if (!skill?.interactionKind) {
    return runtime;
  }

  return {
    activation: {
      skillId: skill.skillId,
      interactionKind: skill.interactionKind,
      selectedTargetSeat: null,
      selectedTileId: null,
      selectedMeldIndex: null,
    },
  };
}

export function closeSkillActivation(_runtime: SkillRuntimeState): SkillRuntimeState {
  return {
    activation: null,
  };
}

export function updateSkillActivationSelection(
  runtime: SkillRuntimeState,
  patch: Partial<Pick<SkillActivationState, 'selectedTargetSeat' | 'selectedTileId' | 'selectedMeldIndex'>>,
): SkillRuntimeState {
  if (!runtime.activation) {
    return runtime;
  }

  return {
    activation: {
      ...runtime.activation,
      ...patch,
    },
  };
}

export function confirmSkillActivation(_runtime: SkillRuntimeState, _sessionState: SessionState): SkillRuntimeState {
  return {
    activation: null,
  };
}

export function buildSkillActivationRequest(runtime: SkillRuntimeState): {
  actionType: `skill:${string}`;
  tileIds: string[];
} | null {
  const activation = runtime.activation;
  if (!activation) {
    return null;
  }

  const tileIds =
    activation.interactionKind === 'select_target' && activation.selectedTargetSeat !== null
      ? [`seat:${activation.selectedTargetSeat}`]
      : activation.interactionKind === 'select_hand_tile' && activation.selectedTileId
        ? [activation.selectedTileId]
        : activation.interactionKind === 'select_meld' && activation.selectedMeldIndex !== null
          ? [`meld:${activation.selectedMeldIndex}`]
          : [];

  return {
    actionType: `skill:${activation.skillId}`,
    tileIds,
  };
}

function buildTargetChoices(viewModel: BattleViewModel, activation: SkillActivationState) {
  return viewModel.players
    .filter((player) => !player.isLocal && typeof player.absoluteSeat === 'number')
    .map((player) => ({
      id: String(player.absoluteSeat),
      label: player.name,
      description: player.wind,
      selected: player.absoluteSeat === activation.selectedTargetSeat,
    }));
}

function buildHandChoices(viewModel: BattleViewModel, activation: SkillActivationState): SkillActivationTileChoiceView[] {
  return viewModel.localHand.map((tile) => ({
    tileId: tile.tileId,
    code: tile.code,
    selected: tile.tileId === activation.selectedTileId,
  }));
}

function buildMeldChoices(viewModel: BattleViewModel, activation: SkillActivationState): SkillActivationMeldChoiceView[] {
  const localPlayer = viewModel.players.find((player) => player.isLocal);
  return (localPlayer?.melds ?? []).map((meld, index) => ({
    index,
    label: `副露 ${index + 1}`,
    tiles: meld,
    selected: index === activation.selectedMeldIndex,
  }));
}

function canConfirmActivation(activation: SkillActivationState) {
  switch (activation.interactionKind) {
    case 'confirm':
    case 'preview_wall':
      return true;
    case 'select_target':
      return activation.selectedTargetSeat !== null;
    case 'select_hand_tile':
      return Boolean(activation.selectedTileId);
    case 'select_meld':
      return activation.selectedMeldIndex !== null;
    default:
      return false;
  }
}

function buildActivationView(
  viewModel: BattleViewModel,
  sessionState: SessionState,
  runtime: SkillRuntimeState,
): SkillActivationView | null {
  const activation = runtime.activation;
  if (!activation) {
    return null;
  }

  const skill = getLocalEquippedSkills(sessionState).find(
    (candidate) =>
      candidate.skill_id === activation.skillId &&
      candidate.type === 'active' &&
      candidate.can_activate_now,
  );
  const mappedSkill = toPlayerSkillView(skill);
  if (!mappedSkill) {
    return null;
  }

  return {
    skill: mappedSkill,
    kind: activation.interactionKind,
    title: `${mappedSkill.name} · 发动技能`,
    description: mappedSkill.interactionHint ?? mappedSkill.detail ?? mappedSkill.summary,
    confirmLabel: '发动技能',
    canConfirm: canConfirmActivation(activation),
    targetChoices:
      activation.interactionKind === 'select_target' ? buildTargetChoices(viewModel, activation) : undefined,
    handChoices:
      activation.interactionKind === 'select_hand_tile' ? buildHandChoices(viewModel, activation) : undefined,
    meldChoices:
      activation.interactionKind === 'select_meld' ? buildMeldChoices(viewModel, activation) : undefined,
    previewTiles:
      activation.interactionKind === 'preview_wall'
        ? buildPreviewTiles(sessionState, activation.skillId)
        : undefined,
  };
}

function withActivateSkillAction(actions: BattleActionView[], skill: PlayerSkillView | null): BattleActionView[] {
  if (!skill?.canActivateNow) {
    return actions;
  }

  if (actions.some((action) => action.id === 'activate_skill')) {
    return actions;
  }

  const nextActions = actions.slice();
  const passIndex = nextActions.findIndex((action) => action.id === 'pass');
  const skillAction: BattleActionView = {
    id: 'activate_skill',
    label: '发动技能',
    enabled: true,
    emphasis: 'medium',
  };

  if (passIndex >= 0) {
    nextActions.splice(passIndex, 0, skillAction);
  } else {
    nextActions.push(skillAction);
  }

  return nextActions;
}

export function createSkillEnhancedBattleViewModel(
  viewModel: BattleViewModel,
  sessionState: SessionState,
  runtime: SkillRuntimeState,
): BattleViewModel {
  if (!isSkillMode(sessionState)) {
    return {
      ...viewModel,
      skillActivation: null,
    };
  }

  const activatableSkill = getLocalActivatableSkill(sessionState);
  return {
    ...viewModel,
    actions:
      viewModel.skillSelection || viewModel.mode !== 'my_turn'
        ? viewModel.actions
        : withActivateSkillAction(viewModel.actions, activatableSkill),
    skillActivation: buildActivationView(viewModel, sessionState, runtime),
  };
}
