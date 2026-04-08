import { formatTileName } from './tileNames';
import type {
  BattleActionView,
  BattleViewModel,
  PlayerSkillView,
  Seat,
  SessionState,
  SkillActivationChoiceView,
  SkillActivationView,
  SkillChoiceView,
  SkillInteractionKind,
  SkillRarity,
} from '../types/match';

const SKILL_SELECTION_DURATION_MS = 30_000;
const SKILL_TOOLTIP_DELAY_MS = 500;

export const PLAYER_SKILL_TOOLTIP_DELAY_MS = SKILL_TOOLTIP_DELAY_MS;

interface SkillCatalogEntry {
  id: string;
  name: string;
  summary: string;
  rarityValues: Record<SkillRarity, string>;
  type: 'active' | 'passive';
  interactionKind: SkillInteractionKind | null;
  interactionHint: string | null;
  tags: string[];
}

interface SkillChoiceState {
  skillId: string;
  rarity: SkillRarity;
}

interface SkillCycleDecisionState {
  cycleKey: string;
  cycleLabel: string;
  deadlineAt: string;
  options: SkillChoiceState[];
  status: 'pending' | 'selected' | 'declined';
  selectedSkillId: string | null;
  selectedRarity: SkillRarity | null;
  usedRoundIds: string[];
}

interface SkillActivationState {
  cycleKey: string;
  roundId: string;
  skillId: string;
  rarity: SkillRarity;
  selectedTargetSeat: number | null;
  selectedTileId: string | null;
  selectedMeldIndex: number | null;
}

export interface SkillRuntimeState {
  decisionsByCycle: Record<string, SkillCycleDecisionState>;
  activation: SkillActivationState | null;
}

interface SkillRoundContext {
  cycleKey: string;
  cycleLabel: string;
  roundId: string;
  roundWind: string;
  handNumber: number;
  localSeat: number;
  selectionWindowOpen: boolean;
}

const SKILL_CATALOG: SkillCatalogEntry[] = [
  {
    id: '01',
    name: '瞒天过海',
    summary: '达成门前清胡牌额外加分；若本局存在副露胡牌则扣分。',
    rarityValues: { common: '+2 / -1', rare: '+5 / -3', epic: '+12 / -8' },
    type: 'passive',
    interactionKind: null,
    interactionHint: null,
    tags: ['门清', '结算'],
  },
  {
    id: '02',
    name: '围魏救赵',
    summary: '输分时减免损失；作为代价，你本局胡牌得分也会降低。',
    rarityValues: { common: '免1 / 扣1', rare: '免3 / 扣2', epic: '免8 / 扣5' },
    type: 'passive',
    interactionKind: null,
    interactionHint: null,
    tags: ['止损', '防守'],
  },
  {
    id: '03',
    name: '借刀杀人',
    summary: '胡牌时每有一组副露加分；若门清胡牌则反向扣分。',
    rarityValues: { common: '+1 / -1', rare: '+2 / -2', epic: '+4 / -3' },
    type: 'passive',
    interactionKind: null,
    interactionHint: null,
    tags: ['副露', '结算'],
  },
  {
    id: '04',
    name: '以逸待劳',
    summary: '偏向后程发力：后段胡牌加分，前段胡牌扣分。',
    rarityValues: { common: '+2 / -1', rare: '+5 / -3', epic: '+12 / -8' },
    type: 'passive',
    interactionKind: null,
    interactionHint: null,
    tags: ['牌墙', '时机'],
  },
  {
    id: '05',
    name: '趁火打劫',
    summary: '有人报听时胡牌加分；若全场无人报听则会被反噬。',
    rarityValues: { common: '+3 / -1', rare: '+6 / -3', epic: '+15 / -8' },
    type: 'passive',
    interactionKind: null,
    interactionHint: null,
    tags: ['局势', '结算'],
  },
  {
    id: '06',
    name: '声东击西',
    summary: '在自己回合发动，窥视牌墙尾部的后续来张，但胡牌门槛会提高。',
    rarityValues: { common: '看1张 / +1番', rare: '看2张 / +1番', epic: '看3张 / +2番' },
    type: 'active',
    interactionKind: 'preview_wall',
    interactionHint: '发动后会在面板中预览牌墙尾部情报。',
    tags: ['信息', '摸牌'],
  },
  {
    id: '07',
    name: '无中生有',
    summary: '选择一张手牌发动置换，请求后端将其随机替换为更有效的进张。',
    rarityValues: { common: '成功+2 / 失败-1', rare: '成功+5 / 失败-3', epic: '成功+12 / 失败-8' },
    type: 'active',
    interactionKind: 'select_hand_tile',
    interactionHint: '发动时从当前手牌中点选一张作为置换目标。',
    tags: ['功能', '手牌'],
  },
  {
    id: '08',
    name: '暗度陈仓',
    summary: '在自己回合指定一名对手，申请侦察其当前手牌情报。',
    rarityValues: { common: '侦察 / 扣1', rare: '侦察 / 扣3', epic: '侦察 / 扣6' },
    type: 'active',
    interactionKind: 'select_target',
    interactionHint: '发动时从其他三家中选择一名作为侦察目标。',
    tags: ['信息', '目标'],
  },
  {
    id: '09',
    name: '隔岸观火',
    summary: '荒庄时获得补偿；只要有人胡牌（含自己）则扣分。',
    rarityValues: { common: '+3 / -1', rare: '+6 / -3', epic: '+15 / -8' },
    type: 'passive',
    interactionKind: null,
    interactionHint: null,
    tags: ['流局', '止损'],
  },
  {
    id: '10',
    name: '笑里藏刀',
    summary: '胡牌包含幺九结构时加分；若全是中张则扣分。',
    rarityValues: { common: '+3 / -1', rare: '+6 / -3', epic: '+15 / -8' },
    type: 'passive',
    interactionKind: null,
    interactionHint: null,
    tags: ['牌型', '幺九'],
  },
  {
    id: '11',
    name: '李代桃僵',
    summary: '被动抵消一次放铳损失，但会压低之后的胡牌收益。',
    rarityValues: { common: '抵3 / 扣2', rare: '抵8 / 扣5', epic: '抵15 / 扣10' },
    type: 'passive',
    interactionKind: null,
    interactionHint: null,
    tags: ['防守', '被动'],
  },
  {
    id: '12',
    name: '顺手牵羊',
    summary: '截胡或抢杠胡额外加分；普通胡牌反而扣分。',
    rarityValues: { common: '+4 / -2', rare: '+8 / -4', epic: '+16 / -10' },
    type: 'passive',
    interactionKind: null,
    interactionHint: null,
    tags: ['截胡', '抢杠胡'],
  },
  {
    id: '13',
    name: '打草惊蛇',
    summary: '有人杠牌后再胡会加分；若整局无人杠牌则扣分。',
    rarityValues: { common: '+2 / -1', rare: '+5 / -3', epic: '+12 / -8' },
    type: 'passive',
    interactionKind: null,
    interactionHint: null,
    tags: ['杠牌', '局势'],
  },
  {
    id: '14',
    name: '借尸还魂',
    summary: '胡熟张加分；胡绝对生张扣分。',
    rarityValues: { common: '+2 / -1', rare: '+5 / -3', epic: '+12 / -8' },
    type: 'passive',
    interactionKind: null,
    interactionHint: null,
    tags: ['熟张', '读牌'],
  },
  {
    id: '15',
    name: '调虎离山',
    summary: '断幺胡牌加分；牌型含幺九或字牌则扣分。',
    rarityValues: { common: '+2 / -1', rare: '+5 / -3', epic: '+12 / -8' },
    type: 'passive',
    interactionKind: null,
    interactionHint: null,
    tags: ['断幺', '牌型'],
  },
  {
    id: '16',
    name: '欲擒故纵',
    summary: '故意放弃一次点炮后，若三巡内再胡可获额外收益；否则扣分。',
    rarityValues: { common: '+3 / -2', rare: '+6 / -4', epic: '+15 / -10' },
    type: 'passive',
    interactionKind: null,
    interactionHint: null,
    tags: ['时机', '追击'],
  },
  {
    id: '17',
    name: '抛砖引玉',
    summary: '本局打出过5后胡牌加分；若最终牌型仍含5则扣分。',
    rarityValues: { common: '+2 / -1', rare: '+5 / -3', epic: '+12 / -8' },
    type: 'passive',
    interactionKind: null,
    interactionHint: null,
    tags: ['数牌', '结构'],
  },
  {
    id: '18',
    name: '擒贼擒王',
    summary: '高番胡牌加分；仅擦线和牌会扣分。',
    rarityValues: { common: '+3 / -1', rare: '+6 / -3', epic: '+15 / -8' },
    type: 'passive',
    interactionKind: null,
    interactionHint: null,
    tags: ['番数', '上限'],
  },
  {
    id: '19',
    name: '釜底抽薪',
    summary: '最后10张牌内胡牌加分；前30张内胡牌扣分。',
    rarityValues: { common: '+3 / -1', rare: '+6 / -3', epic: '+15 / -8' },
    type: 'passive',
    interactionKind: null,
    interactionHint: null,
    tags: ['牌墙', '残局'],
  },
  {
    id: '20',
    name: '混水摸鱼',
    summary: '乱战局面加分；纯色平稳局则扣分。',
    rarityValues: { common: '+2 / -1', rare: '+5 / -3', epic: '+12 / -8' },
    type: 'passive',
    interactionKind: null,
    interactionHint: null,
    tags: ['局势', '花色'],
  },
  {
    id: '21',
    name: '金蝉脱壳',
    summary: '听牌后可在危险来张时弃和止损；正常胡牌收益降低。',
    rarityValues: { common: '少扣2 / 减1', rare: '少扣5 / 减3', epic: '少扣12 / 减8' },
    type: 'passive',
    interactionKind: null,
    interactionHint: null,
    tags: ['止损', '听牌'],
  },
  {
    id: '22',
    name: '关门捉贼',
    summary: '嵌张、边张、单钓胡牌加分；多面听胡牌扣分。',
    rarityValues: { common: '+2 / -1', rare: '+5 / -3', epic: '+12 / -8' },
    type: 'passive',
    interactionKind: null,
    interactionHint: null,
    tags: ['听型', '牌理'],
  },
  {
    id: '23',
    name: '远交近攻',
    summary: '鼓励针对对家胡牌：座次越远收益越高。',
    rarityValues: { common: '对家+3 / 旁-1', rare: '对家+6 / 旁-3', epic: '对家+15 / 旁-8' },
    type: 'passive',
    interactionKind: null,
    interactionHint: null,
    tags: ['座次', '结算'],
  },
  {
    id: '24',
    name: '假道伐虢',
    summary: '杠上开花加分；若最终没有杠牌则扣分。',
    rarityValues: { common: '+3 / -1', rare: '+6 / -3', epic: '+15 / -8' },
    type: 'passive',
    interactionKind: null,
    interactionHint: null,
    tags: ['杠上开花', '杠牌'],
  },
  {
    id: '25',
    name: '偷梁换柱',
    summary: '在自己回合选择一组副露，向后端请求将其收回手牌并转为暗牌。',
    rarityValues: { common: '扣3', rare: '扣6', epic: '扣12' },
    type: 'active',
    interactionKind: 'select_meld',
    interactionHint: '发动时从自己已亮出的副露中选择一组进行回收。',
    tags: ['功能', '副露'],
  },
  {
    id: '26',
    name: '指桑骂槐',
    summary: '打出字牌后若又摸回相同字牌可加分；整局未触发则扣分。',
    rarityValues: { common: '+2 / -1', rare: '+5 / -3', epic: '+12 / -8' },
    type: 'passive',
    interactionKind: null,
    interactionHint: null,
    tags: ['字牌', '摸牌'],
  },
  {
    id: '27',
    name: '假痴不癫',
    summary: '降低起和门槛，但整体收益也会被轻微削弱。',
    rarityValues: { common: '7番 / 扣2', rare: '6番 / 扣5', epic: '5番 / 扣10' },
    type: 'passive',
    interactionKind: null,
    interactionHint: null,
    tags: ['起和门槛', '风险'],
  },
  {
    id: '28',
    name: '上屋抽梯',
    summary: '听牌时越接近牌墙末端收益越高；过早胡牌会扣分。',
    rarityValues: { common: '+2 / -1', rare: '+5 / -3', epic: '+12 / -8' },
    type: 'passive',
    interactionKind: null,
    interactionHint: null,
    tags: ['听牌', '牌墙'],
  },
  {
    id: '29',
    name: '树上开花',
    summary: '摸到花牌会提升结算；整局没摸到花牌反而扣分。',
    rarityValues: { common: '每张+1 / -1', rare: '每张+2 / -2', epic: '每张+3 / -5' },
    type: 'passive',
    interactionKind: null,
    interactionHint: null,
    tags: ['花牌', '结算'],
  },
  {
    id: '30',
    name: '反客为主',
    summary: '非庄家胡庄家点炮时加分；庄家胡牌则会额外扣分。',
    rarityValues: { common: '+2 / -1', rare: '+5 / -3', epic: '+12 / -8' },
    type: 'passive',
    interactionKind: null,
    interactionHint: null,
    tags: ['庄家', '座次'],
  },
  {
    id: '31',
    name: '美人计',
    summary: '字牌刻子或将牌越多收益越高；完全无字则扣分。',
    rarityValues: { common: '每组+1 / -1', rare: '每组+3 / -3', epic: '每组+6 / -8' },
    type: 'passive',
    interactionKind: null,
    interactionHint: null,
    tags: ['字牌', '牌型'],
  },
  {
    id: '32',
    name: '空城计',
    summary: '特定暗牌数量的听牌形态加分；暗牌过多时胡牌扣分。',
    rarityValues: { common: '+3 / -1', rare: '+6 / -3', epic: '+15 / -8' },
    type: 'passive',
    interactionKind: null,
    interactionHint: null,
    tags: ['暗牌', '听型'],
  },
  {
    id: '33',
    name: '反间计',
    summary: '自己打出的牌若常被他家吃碰会加分；无人理会则扣分。',
    rarityValues: { common: '+2 / -1', rare: '+5 / -3', epic: '+12 / -8' },
    type: 'passive',
    interactionKind: null,
    interactionHint: null,
    tags: ['博弈', '弃牌'],
  },
  {
    id: '34',
    name: '苦肉计',
    summary: '总分落后时胡牌加分；领先时胡牌反而扣分。',
    rarityValues: { common: '+2 / -1', rare: '+5 / -3', epic: '+12 / -8' },
    type: 'passive',
    interactionKind: null,
    interactionHint: null,
    tags: ['分差', '逆风'],
  },
  {
    id: '35',
    name: '连环计',
    summary: '连续胡牌收益递增，但一旦断连会立刻反噬。',
    rarityValues: { common: '+2 / -2', rare: '+5 / -4', epic: '+12 / -10' },
    type: 'passive',
    interactionKind: null,
    interactionHint: null,
    tags: ['连胜', '风险'],
  },
  {
    id: '36',
    name: '走为上计',
    summary: '在自己回合主动申请流局止损，但会压低你下一局的胡牌收益。',
    rarityValues: { common: '减2分', rare: '减5分', epic: '减12分' },
    type: 'active',
    interactionKind: 'confirm',
    interactionHint: '发动后会直接向后端发送“强制流局”请求。',
    tags: ['功能', '止损'],
  },
];

const SKILL_BY_ID = new Map(SKILL_CATALOG.map((skill) => [skill.id, skill]));
const WIND_COPY: Record<string, string> = {
  east: '东',
  south: '南',
  west: '西',
  north: '北',
};
const RARITY_LABELS: Record<SkillRarity, string> = {
  common: '普通',
  rare: '稀有',
  epic: '史诗',
};
const RARITY_TONES: Record<SkillRarity, PlayerSkillView['tone']> = {
  common: 'jade',
  rare: 'azure',
  epic: 'violet',
};
const SKILL_ACTION_PRIORITY = 4;

export function createInitialSkillRuntimeState(): SkillRuntimeState {
  return {
    decisionsByCycle: {},
    activation: null,
  };
}

export function syncSkillRuntimeWithSession(runtime: SkillRuntimeState, sessionState: SessionState): SkillRuntimeState {
  const context = getSkillRoundContext(sessionState);
  let nextRuntime = runtime;

  if (!context) {
    if (runtime.activation) {
      nextRuntime = {
        ...runtime,
        activation: null,
      };
    }
    return nextRuntime;
  }

  const currentDecision = runtime.decisionsByCycle[context.cycleKey];
  if (context.selectionWindowOpen && !currentDecision) {
    nextRuntime = {
      ...nextRuntime,
      decisionsByCycle: {
        ...nextRuntime.decisionsByCycle,
        [context.cycleKey]: {
          cycleKey: context.cycleKey,
          cycleLabel: context.cycleLabel,
          deadlineAt: new Date(Date.now() + SKILL_SELECTION_DURATION_MS).toISOString(),
          options: drawSkillChoices(),
          status: 'pending',
          selectedSkillId: null,
          selectedRarity: null,
          usedRoundIds: [],
        },
      },
    };
  }

  if (nextRuntime.activation) {
    const isSameRound = nextRuntime.activation.roundId === context.roundId;
    const isSameCycle = nextRuntime.activation.cycleKey === context.cycleKey;
    const activeDecision = nextRuntime.decisionsByCycle[context.cycleKey];
    const hasUsableSkill = Boolean(activeDecision && activeDecision.status === 'selected');

    if (!isSameRound || !isSameCycle || !hasUsableSkill) {
      nextRuntime = {
        ...nextRuntime,
        activation: null,
      };
    }
  }

  return nextRuntime;
}

export function declineCurrentSkillOffer(runtime: SkillRuntimeState, sessionState: SessionState): SkillRuntimeState {
  const context = getSkillRoundContext(sessionState);
  if (!context) {
    return runtime;
  }

  const decision = runtime.decisionsByCycle[context.cycleKey];
  if (!decision || decision.status !== 'pending') {
    return runtime;
  }

  return {
    ...runtime,
    decisionsByCycle: {
      ...runtime.decisionsByCycle,
      [context.cycleKey]: {
        ...decision,
        status: 'declined',
      },
    },
    activation: null,
  };
}

export function selectSkillForCurrentCycle(runtime: SkillRuntimeState, sessionState: SessionState, skillId: string): SkillRuntimeState {
  const context = getSkillRoundContext(sessionState);
  if (!context) {
    return runtime;
  }

  const decision = runtime.decisionsByCycle[context.cycleKey];
  if (!decision || decision.status !== 'pending') {
    return runtime;
  }

  const selectedChoice = decision.options.find((option) => option.skillId === skillId);
  if (!selectedChoice) {
    return runtime;
  }

  return {
    ...runtime,
    decisionsByCycle: {
      ...runtime.decisionsByCycle,
      [context.cycleKey]: {
        ...decision,
        status: 'selected',
        selectedSkillId: selectedChoice.skillId,
        selectedRarity: selectedChoice.rarity,
      },
    },
    activation: null,
  };
}

export function openSkillActivation(runtime: SkillRuntimeState, sessionState: SessionState): SkillRuntimeState {
  const activeSkill = getCurrentSelectedSkill(runtime, sessionState);
  if (!activeSkill || activeSkill.skill.type !== 'active' || !activeSkill.canActivate || !activeSkill.context) {
    return runtime;
  }

  return {
    ...runtime,
    activation: {
      cycleKey: activeSkill.context.cycleKey,
      roundId: activeSkill.context.roundId,
      skillId: activeSkill.skill.id,
      rarity: activeSkill.rarity,
      selectedTargetSeat: null,
      selectedTileId: null,
      selectedMeldIndex: null,
    },
  };
}

export function closeSkillActivation(runtime: SkillRuntimeState): SkillRuntimeState {
  if (!runtime.activation) {
    return runtime;
  }

  return {
    ...runtime,
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
    ...runtime,
    activation: {
      ...runtime.activation,
      ...patch,
    },
  };
}

export function confirmSkillActivation(runtime: SkillRuntimeState, sessionState: SessionState): SkillRuntimeState {
  const activeSkill = getCurrentSelectedSkill(runtime, sessionState);
  const activation = runtime.activation;
  if (!activeSkill || !activation || !activeSkill.context || activation.cycleKey !== activeSkill.context.cycleKey) {
    return runtime;
  }

  const decision = runtime.decisionsByCycle[activeSkill.context.cycleKey];
  if (!decision) {
    return runtime;
  }

  if (!isActivationSelectionComplete(activeSkill.skill, activation)) {
    return runtime;
  }

  const usedRoundIds = decision.usedRoundIds.includes(activeSkill.context.roundId)
    ? decision.usedRoundIds
    : [...decision.usedRoundIds, activeSkill.context.roundId];

  return {
    ...runtime,
    decisionsByCycle: {
      ...runtime.decisionsByCycle,
      [activeSkill.context.cycleKey]: {
        ...decision,
        usedRoundIds,
      },
    },
    activation: null,
  };
}

export function createSkillEnhancedBattleViewModel(
  baseViewModel: BattleViewModel,
  sessionState: SessionState,
  runtime: SkillRuntimeState,
): BattleViewModel {
  const context = getSkillRoundContext(sessionState);
  const activeSkill = getCurrentSelectedSkill(runtime, sessionState);
  const players = baseViewModel.players.map((player) => {
    if (!player.isLocal || !activeSkill) {
      return {
        ...player,
        skill: player.skill ?? null,
      };
    }

    return {
      ...player,
      skill: toPlayerSkillView(activeSkill.skill, activeSkill.rarity, activeSkill.context, activeSkill.remainingActivationsThisRound),
    };
  });

  let actions = baseViewModel.actions;
  if (activeSkill?.skill.type === 'active' && baseViewModel.mode === 'my_turn') {
    const hasAction = actions.some((action) => action.id === 'activate_skill');
    if (!hasAction) {
      const activateAction: BattleActionView = {
        id: 'activate_skill',
        label: '发动技能',
        enabled: activeSkill.canActivate,
        emphasis: activeSkill.canActivate ? 'high' : 'low',
      };
      actions = [...actions, activateAction].sort((left, right) => getActionPriority(left.id) - getActionPriority(right.id));
    }
  }

  return {
    ...baseViewModel,
    players,
    actions,
    skillSelection: createSkillSelectionView(runtime, sessionState),
    skillActivation: runtime.activation && activeSkill ? createSkillActivationView(baseViewModel, activeSkill, runtime.activation) : null,
  };
}

function createSkillSelectionView(runtime: SkillRuntimeState, sessionState: SessionState) {
  const context = getSkillRoundContext(sessionState);
  if (!context) {
    return null;
  }

  const decision = runtime.decisionsByCycle[context.cycleKey];
  if (!decision || decision.status !== 'pending') {
    return null;
  }

  return {
    cycleKey: decision.cycleKey,
    cycleLabel: decision.cycleLabel,
    deadlineAt: decision.deadlineAt,
    title: `${decision.cycleLabel} · 技能签启`,
    detail: '每种技能持续两局；主动技能每局仅可发动一次，未用次数不会累加。',
    options: decision.options.map((option) => {
      const skill = getSkillById(option.skillId);
      return {
        ...toPlayerSkillView(skill, option.rarity, context, skill.type === 'active' ? 1 : 0),
        cycleKey: decision.cycleKey,
      } satisfies SkillChoiceView;
    }),
  };
}

function createSkillActivationView(
  baseViewModel: BattleViewModel,
  activeSkill: NonNullable<ReturnType<typeof getCurrentSelectedSkill>>,
  activation: SkillActivationState,
): SkillActivationView {
  const skillView = toPlayerSkillView(
    activeSkill.skill,
    activeSkill.rarity,
    activeSkill.context,
    activeSkill.remainingActivationsThisRound,
  );

  const title = `发动技能 · ${activeSkill.skill.name}`;
  const description = buildActivationDescription(activeSkill.skill, activeSkill.rarity);
  const confirmLabel = getActivationConfirmLabel(activeSkill.skill.interactionKind);

  if (activeSkill.skill.interactionKind === 'select_target') {
    const targetChoices = baseViewModel.players
      .filter((player) => !player.isLocal)
      .map((player) => ({
        id: String(player.absoluteSeat ?? player.seat),
        label: player.name,
        description: `${player.wind}位 · ${player.statusText ?? '对局中'}`,
        selected: activation.selectedTargetSeat === player.absoluteSeat,
      })) satisfies SkillActivationChoiceView[];

    return {
      skill: skillView,
      kind: 'select_target',
      title,
      description,
      confirmLabel,
      canConfirm: targetChoices.some((choice) => choice.selected),
      targetChoices,
    };
  }

  if (activeSkill.skill.interactionKind === 'select_hand_tile') {
    const handChoices = baseViewModel.localHand.map((tile) => ({
      tileId: tile.tileId,
      code: tile.code,
      label: formatTileName(tile.code),
      selected: activation.selectedTileId === tile.tileId,
    }));

    return {
      skill: skillView,
      kind: 'select_hand_tile',
      title,
      description,
      confirmLabel,
      canConfirm: handChoices.some((choice) => choice.selected),
      handChoices,
    };
  }

  if (activeSkill.skill.interactionKind === 'select_meld') {
    const localPlayer = baseViewModel.players.find((player) => player.isLocal);
    const meldChoices = (localPlayer?.melds ?? []).map((meld, index) => ({
      index,
      label: `副露 ${index + 1}`,
      tiles: meld,
      selected: activation.selectedMeldIndex === index,
    }));

    return {
      skill: skillView,
      kind: 'select_meld',
      title,
      description,
      confirmLabel,
      canConfirm: meldChoices.some((choice) => choice.selected),
      meldChoices,
    };
  }

  if (activeSkill.skill.interactionKind === 'preview_wall') {
    const previewCount = getPreviewTileCount(activeSkill.rarity);
    return {
      skill: skillView,
      kind: 'preview_wall',
      title,
      description,
      confirmLabel,
      canConfirm: true,
      previewTiles: Array.from({ length: previewCount }, (_, index) => ({
        key: `${activeSkill.skill.id}-${index}`,
        revealedLabel: `尾牌情报 ${index + 1}`,
        hiddenLabel: `待后端同步 · 预览位 ${index + 1}`,
      })),
    };
  }

  return {
    skill: skillView,
    kind: 'confirm',
    title,
    description,
    confirmLabel,
    canConfirm: true,
  };
}

function getCurrentSelectedSkill(runtime: SkillRuntimeState, sessionState: SessionState) {
  const context = getSkillRoundContext(sessionState);
  if (!context) {
    return null;
  }

  const decision = runtime.decisionsByCycle[context.cycleKey];
  if (!decision || decision.status !== 'selected' || !decision.selectedSkillId || !decision.selectedRarity) {
    return null;
  }

  const skill = getSkillById(decision.selectedSkillId);
  const remainingActivationsThisRound = skill.type === 'active' && !decision.usedRoundIds.includes(context.roundId) ? 1 : 0;

  return {
    context,
    decision,
    skill,
    rarity: decision.selectedRarity,
    canActivate: skill.type === 'active' && remainingActivationsThisRound > 0,
    remainingActivationsThisRound,
  };
}

function toPlayerSkillView(
  skill: SkillCatalogEntry,
  rarity: SkillRarity,
  context: SkillRoundContext,
  remainingActivationsThisRound: number,
): PlayerSkillView {
  return {
    skillId: skill.id,
    name: skill.name,
    rarity,
    rarityLabel: RARITY_LABELS[rarity],
    tone: RARITY_TONES[rarity],
    type: skill.type,
    typeLabel: skill.type === 'active' ? '主动技能' : '被动技能',
    summary: skill.summary,
    detail: `${RARITY_LABELS[rarity]}效果：${skill.rarityValues[rarity]}`,
    interactionHint: skill.interactionHint,
    tags: skill.tags,
    cycleLabel: context.cycleLabel,
    remainingRounds: getRemainingRoundsInCycle(context),
    remainingActivationsThisRound,
  };
}

function getSkillRoundContext(sessionState: SessionState): SkillRoundContext | null {
  const snapshot = sessionState.roomSnapshot?.payload;
  const matchState = snapshot?.match_state;
  const privateState = snapshot?.private_state;

  if (
    snapshot?.phase !== 'playing' ||
    !matchState ||
    !privateState ||
    typeof snapshot.local_seat !== 'number' ||
    typeof matchState.hand_number !== 'number' ||
    typeof privateState.round_id !== 'string'
  ) {
    return null;
  }

  const handNumber = matchState.hand_number;
  const cycleStartHand = handNumber % 2 === 1 ? handNumber : handNumber - 1;
  if (cycleStartHand < 1) {
    return null;
  }

  const roundWind = privateState.round_wind ?? matchState.prevailing_wind;
  const cycleLabel = `${WIND_COPY[roundWind] ?? roundWind}${cycleStartHand}~${WIND_COPY[roundWind] ?? roundWind}${Math.min(
    cycleStartHand + 1,
    4,
  )}局`;
  const selectionWindowOpen =
    handNumber % 2 === 1 &&
    privateState.pending_action?.type === 'opening_flowers' &&
    privateState.last_discard == null &&
    privateState.players.every((player) => (player.discards?.length ?? 0) === 0);

  return {
    cycleKey: `${roundWind}-${cycleStartHand}`,
    cycleLabel,
    roundId: privateState.round_id,
    roundWind,
    handNumber,
    localSeat: snapshot.local_seat,
    selectionWindowOpen,
  };
}

function getRemainingRoundsInCycle(context: SkillRoundContext) {
  return context.handNumber % 2 === 1 ? 2 : 1;
}

function drawSkillChoices(): SkillChoiceState[] {
  const pool = [...SKILL_CATALOG];
  const firstIndex = Math.floor(Math.random() * pool.length);
  const [firstSkill] = pool.splice(firstIndex, 1);
  const secondIndex = Math.floor(Math.random() * pool.length);
  const [secondSkill] = pool.splice(secondIndex, 1);

  return [firstSkill, secondSkill].filter(Boolean).map((skill) => ({
    skillId: skill.id,
    rarity: rollSkillRarity(),
  }));
}

function rollSkillRarity(): SkillRarity {
  const value = Math.random();
  if (value < 0.65) {
    return 'common';
  }
  if (value < 0.95) {
    return 'rare';
  }
  return 'epic';
}

function getSkillById(skillId: string) {
  const skill = SKILL_BY_ID.get(skillId);
  if (!skill) {
    throw new Error(`Unknown skill id: ${skillId}`);
  }
  return skill;
}

function getActionPriority(actionId: BattleActionView['id']) {
  if (actionId === 'activate_skill') {
    return SKILL_ACTION_PRIORITY;
  }

  const lookup: Partial<Record<BattleActionView['id'], number>> = {
    hu: 0,
    kong: 1,
    pung: 2,
    chow: 3,
    flower: 5,
    discard: 6,
    pass: 7,
  };

  return lookup[actionId] ?? Number.MAX_SAFE_INTEGER;
}

function buildActivationDescription(skill: SkillCatalogEntry, rarity: SkillRarity) {
  const base = `${skill.summary} 当前${RARITY_LABELS[rarity]}档效果为 ${skill.rarityValues[rarity]}。`;

  if (skill.type === 'active') {
    return `${base} 按当前前端规则，主动技能每局只能发动一次，未使用次数不会累加。`;
  }

  return base;
}

function getActivationConfirmLabel(kind: SkillCatalogEntry['interactionKind']) {
  switch (kind) {
    case 'preview_wall':
      return '开始窥视';
    case 'select_target':
      return '锁定目标';
    case 'select_hand_tile':
      return '提交置换';
    case 'select_meld':
      return '回收副露';
    default:
      return '确认发动';
  }
}

function getPreviewTileCount(rarity: SkillRarity) {
  if (rarity === 'epic') {
    return 3;
  }

  return rarity === 'rare' ? 2 : 1;
}

function isActivationSelectionComplete(skill: SkillCatalogEntry, activation: SkillActivationState) {
  switch (skill.interactionKind) {
    case 'select_target':
      return typeof activation.selectedTargetSeat === 'number';
    case 'select_hand_tile':
      return typeof activation.selectedTileId === 'string';
    case 'select_meld':
      return typeof activation.selectedMeldIndex === 'number';
    default:
      return true;
  }
}
