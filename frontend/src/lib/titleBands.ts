export type PlayerTitle = (typeof TITLE_BANDS)[number]['title'];

type TitleBand = {
  minPoints: number;
  title: string;
  description: string;
};

export const TITLE_BANDS = [
  {
    minPoints: -50,
    title: 'LV1.散财童子',
    description: '分低到像是在做慈善，牌桌上的财神爷，专程给三家送温暖。',
  },
  {
    minPoints: 50,
    title: 'LV2.点炮能手',
    description: '别人胡牌你助攻，精准一发点炮，堪称对手的最佳队友。',
  },
  {
    minPoints: 150,
    title: 'LV3.慈善赌王',
    description: '输得荡气回肠，用实力诠释“重在参与”，善款捐赠量全桌第一。',
  },
  {
    minPoints: 250,
    title: 'LV4.常年陪跑',
    description: '永远在陪打，从未拿过胜果，存在感约等于牌桌背景板。',
  },
  {
    minPoints: 350,
    title: 'LV5.失魂雀士',
    description: '摸牌犹犹豫豫，出牌神游天外，魂儿没带来，分被带走了。',
  },
  {
    minPoints: 450,
    title: 'LV6.迷途麻客',
    description: '知道牌型但不知道方向，经常在“该攻该守”的十字路口迷路。',
  },
  {
    minPoints: 550,
    title: 'LV7.入门雀友',
    description: '终于摸清国标规则，但离赢牌还隔着一整本《麻将高阶走位学》。',
  },
  {
    minPoints: 650,
    title: 'LV8.初露锋芒',
    description: '偶尔能胡出像样的牌，开始让对手觉得“这人有点意思”。',
  },
  {
    minPoints: 750,
    title: 'LV9.稳扎稳打',
    description: '不求惊天动地，只求少点炮、多蹭番，积分渐渐稳住了。',
  },
  {
    minPoints: 850,
    title: 'LV10.牌桌猎手',
    description: '听牌快、胡牌准，一旦嗅到机会就像猎手，迅速拿下战果。',
  },
  {
    minPoints: 950,
    title: 'LV11.运筹帷幄',
    description: '舍牌有章法，做牌有远见，仿佛手里拿着一本剧本在雀桌上导戏。',
  },
  {
    minPoints: 1050,
    title: 'LV12.不败战将',
    description: '败局极少出现，连续获胜已成常态，属于让人不想匹配的存在。',
  },
  {
    minPoints: 1150,
    title: 'LV13.雀坛传说',
    description: '打法已成谈资，排名名震一方，普通雀友在茶馆里会提到你的大名。',
  },
  {
    minPoints: 1250,
    title: 'LV14.封神雀圣',
    description: '近乎神化的控局能力，坐上桌就自带压迫光环，只差一个正式加冕。',
  },
  {
    minPoints: 1350,
    title: 'LV15.至尊雀神',
    description: '国标之巅，俯瞰众生。你的存在本身就是对“运气”二字的不屑。',
  },
] as const satisfies readonly TitleBand[];

export function titleForPoints(points: number): string {
  return titleBandForPoints(points).title;
}

export function titleDescriptionForTitle(title: string): string {
  return TITLE_BANDS.find((band) => band.title === title)?.description ?? '暂无公开简介';
}

export function titleRank(title: string): number {
  const index = TITLE_BANDS.findIndex((band) => band.title === title);
  return index >= 0 ? index : 6;
}

function titleBandForPoints(points: number): TitleBand {
  for (let index = TITLE_BANDS.length - 1; index >= 0; index -= 1) {
    if (points >= TITLE_BANDS[index].minPoints) {
      return TITLE_BANDS[index];
    }
  }
  return TITLE_BANDS[0];
}
