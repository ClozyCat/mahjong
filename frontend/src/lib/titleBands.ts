export type PlayerTitle = (typeof TITLE_BANDS)[number]['title'];

type TitleBand = {
  minPoints: number;
  title: string;
  description: string;
};

export const TITLE_BANDS = [
  {
    minPoints: -50,
    title: '全自动点炮机',
    description: '牌桌上的活菩萨。只要坐上牌桌，唯一的作用就是把自己的分精准地喂进别人的嘴里。',
  },
  {
    minPoints: 50,
    title: '首席散财童子',
    description: '主打一个陪伴。输赢已经不重要了，主要是喜欢看着别人赢钱时开心的笑容。',
  },
  {
    minPoints: 150,
    title: '国标八番困难户',
    description:
      '对国标“起和八番”的门槛有着深深的恐惧，好不容易听牌了，一算番数只有可怜的六番，终生在及格线边缘挣扎。',
  },
  {
    minPoints: 250,
    title: '视力测试员',
    description: '打麻将对他们来说只是一项简单的物理运动：把牌摸起来，看清花色，然后打出去。不包含任何脑力计算。',
  },
  {
    minPoints: 350,
    title: '薛定谔的听牌',
    description: '别人根本猜不透他有没有听牌，因为甚至连他自己都不知道自己听了什么、差几番。',
  },
  {
    minPoints: 450,
    title: '间歇性好运携带者',
    description: '毫无技术可言，能赢全靠发牌员心情好。一旦运气用光，瞬间跌回“全自动点炮机”。',
  },
  {
    minPoints: 550,
    title: '熟练的码牌工',
    description: '已经脱离了新手的低级趣味，不仅码牌速度快，甚至偶尔还能看懂别人在做什么牌。',
  },
  {
    minPoints: 650,
    title: '弹性拆牌艺术家',
    description:
      '深谙“好死不如赖活着”的麻将哲学。前一秒还在雄心勃勃地规划大牌，下一秒察觉到危险，立刻就能把一手好牌拆得连亲妈都不认识。怂得行云流水，退得理直气壮。',
  },
  {
    minPoints: 750,
    title: '牌池人体扫描仪',
    description:
      '双眼犹如X光，死死盯着牌河里的每一张废牌。想从他眼皮子底下骗吃骗碰？不存在的。他不仅知道你想要什么牌，甚至还能反手给你喂一口难受的“毒药”。',
  },
  {
    minPoints: 850,
    title: '精准控分专家',
    description: '国标麻将的精算师。手里捏着无数种组合方式，总能以最刁钻的角度、凑出最性价比的番数拿下对局。',
  },
  {
    minPoints: 950,
    title: '牌桌读心者',
    description: '你的一个眼神，一次犹豫，他就已经知道你听的是哪张牌了。在他面前，其他玩家仿佛是透明的。',
  },
  {
    minPoints: 1050,
    title: '降维打击操盘手',
    description: '别人是在打麻将，他是在做数学建模。通过极强的逻辑推理，将对手玩弄于股掌之间。',
  },
  {
    minPoints: 1150,
    title: '人形量子算番机',
    description: '摸牌的瞬间，大脑已经计算完了国标麻将所有可能的番种组合和胡牌概率。算力之强，令服务器汗颜。',
  },
  {
    minPoints: 1250,
    title: '气运与实力之主',
    description: '科学与玄学的完美结合。不仅算无遗策，连老天爷都站在他这边，想摸什么就来什么。',
  },
  {
    minPoints: 1350,
    title: '言出法随真雀神',
    description: '凡人不可直视的巅峰境界。他坐庄，那是神在审判；他打牌，那是神在赐教。牌桌之上，他就是唯一的真理。',
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
