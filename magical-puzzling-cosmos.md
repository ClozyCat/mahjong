# Three.js 3D 麻将前端重构方案

## Context

将现有 React+CSS DOM 渲染的麻将前端完整替换为 Three.js 3D 风格渲染，保留 App.tsx 中所有业务逻辑、WebSocket 通信和状态管理。目标效果参考原型图：国潮风格水墨背景、3D 视角棋盘、象牙色立体牌块。

技术选型：
- **React Three Fiber (R3F)** 作为 Three.js 的 React 集成层
- **混合渲染**：3D 场景(棋盘+牌)使用 Three.js Canvas，交互按钮/面板保留 HTML 覆盖层
- **一次性完整实现**，包含所有动画

---

## 依赖安装

```bash
cd frontend
npm install @react-three/fiber @react-three/drei three @types/three @react-spring/three
```

---

## 新增文件结构

```
frontend/src/components/three-battle/
  ThreeBattleScreen.tsx         # 主包装器，接受与 BattleScreen 相同的 props
  scene/
    MahjongScene.tsx            # R3F Canvas 根组件，相机+环境设置
    SceneLighting.tsx           # 光照（环境光+方向光+点光源）
    TableSurface.tsx            # 绿色桌面+木质边框
    MahjongTile3D.tsx           # 单块 3D 麻将牌（可点击）
    HandRack3D.tsx              # 本地玩家手牌区域
    OpponentRack3D.tsx          # 对手暗牌排列
    MeldRack3D.tsx              # 副露区（吃碰杠）
    DiscardRiver3D.tsx          # 弃牌河区域
    CenterInfo3D.tsx            # 中央局数/风向/剩余牌数指示器
    WallTiles3D.tsx             # 牌墙（装饰性）
  overlays/
    ActionDockOverlay.tsx       # 吃/碰/杠/胡/过 HTML 按钮层
    PlayerInfoOverlay.tsx       # 各方玩家分数+头像 HTML 面板
    TopBarOverlay.tsx           # 顶部匹配信息栏
  hooks/
    useTileTextures.ts          # 预加载所有牌面纹理
    useSpringTile.ts            # 牌的弹簧动画辅助 hook
```

---

## 修改文件

- `frontend/src/App.tsx` — 将 `<BattleScreen>` 替换为 `<ThreeBattleScreen>`（props 完全相同）
- 保留所有原有组件和 CSS（功能兜底/旁路）

---

## 3D 场景设计

### 相机
```
position: [0, 13, 9]   // 从斜上方俯视
lookAt:   [0, 0, 0]
fov:      55
near:     0.1, far: 100
```

### 桌面布局（单位：Three.js 世界坐标）
```
table plane: 22×22 units at y=0, green felt
frame: 4× BoxGeometry boards at table edges, wood texture

Tile size: width=0.9, height=1.2, depth=0.3

玩家位置（底=本地，顺时针=右/上/左）：
  bottom: z=9.5,  hand spread along x-axis, tiles face camera
  top:    z=-9.5, tiles rotated 180° around Y, face camera
  left:   x=-9.5, tiles rotated 90° around Y
  right:  x=9.5,  tiles rotated -90° around Y

弃牌区（各方 3 列 × 6 行 网格）：
  bottom: z=4~7.5, x=-2.5~2.5
  top:    z=-4~-7.5, x=-2.5~2.5
  left:   x=-4~-7.5, z=-2.5~2.5
  right:  x=4~7.5, z=-2.5~2.5

副露区（各方玩家旁侧）：
  紧邻手牌，向右延伸（相对本人）

中央面板：
  y=0.05, 中心 [0,0,0], 3×3 单位装饰方板
```

### 光照
```
AmbientLight:     intensity=0.6, color=#fff8e7 (暖光)
DirectionalLight: intensity=1.2, position=[5,12,5], castShadow
PointLight:       intensity=0.8, position=[0,6,0], 桌面补光
```

---

## 关键组件实现细节

### MahjongTile3D.tsx
```typescript
// BoxGeometry(0.9, 0.3, 1.2) — Three.js 中 Y 轴向上，牌竖立放置时:
// width=0.9(X), depth=0.3(Z), height=1.2(Y)
// 材质数组(6面): [右,左,上,下,正面,背面]
// 正面(index 4): SVG 纹理 via CanvasTexture
// 背面/侧面: 象牙色 MeshStandardMaterial

// Props:
interface MahjongTile3DProps {
  tileCode: string        // e.g. 'w1', 'east'
  position: [x,y,z]
  rotation?: [x,y,z]
  isFaceDown?: boolean
  isSelected?: boolean    // Y轴弹起 +0.4
  isHighlighted?: boolean // emissive glow
  isLastDiscard?: boolean // 橙色高亮
  isDisabled?: boolean    // 透明度降低
  onClick?: () => void
  onDoubleClick?: () => void
}

// 动画: useSpring from @react-spring/three
// selected → position.y += 0.4, scaleY 微弹跳
```

### useTileTextures.ts
```typescript
// 使用 tileAssets.ts 中已有的 getTileAsset(code) 获取 SVG 内容
// 将 SVG 绘制到 <canvas> → THREE.CanvasTexture
// 预加载所有 42 种牌面纹理
// 返回 Map<tileCode, THREE.Texture>
// 复用 tileAssets.ts: getTileAsset(), assetUrl()
```

### HandRack3D.tsx（本地手牌）
```typescript
// 牌竖立，面朝相机，均匀排布
// 间距 1.0 unit，居中于 x=0
// 选中牌: position.y += 0.4
// 点击 → onTileSelect(tileId)
// 双击 → onTileDoubleClick(tileId)
// 数据源: viewModel.localHand
```

### DiscardRiver3D.tsx（弃牌河）
```typescript
// 每个席位独立组件，接收 discards 数组
// 按 6列×3行排列（或按方向翻转）
// 最后一张弃牌高亮显示（isLastDiscard）
// 牌平放于桌面（rotation.x = -Math.PI/2）
```

### CenterInfo3D.tsx（中央指示器）
```typescript
// 使用 @react-three/drei 的 <Html> 组件在 3D 空间嵌入 DOM
// 或使用 TextGeometry 渲染文字
// 显示：局数文字、风向汉字、剩余张数
// 绕 Y 轴指向当前出牌方向（旋转箭头）
```

### ActionDockOverlay.tsx（HTML 覆盖层）
```typescript
// 绝对定位在 canvas 底部
// 复用现有 BottomActionDock.tsx 的按钮逻辑（吃碰杠胡过）
// 但不渲染手牌（手牌在 3D 场景中）
// 保留 ClaimCandidates 面板
// 保留 ReadyHand/Insight 信息
```

### PlayerInfoOverlay.tsx（HTML 覆盖层）
```typescript
// 四个角落绝对定位的玩家信息卡片
// 显示：头像、昵称、分数、连接状态
// 位置：
//   bottom: 左下或底部居中
//   top: 顶部中央
//   left: 左侧居中
//   right: 右侧居中
```

---

## 动画系统

### 出牌动画（弃牌飞行）
```
1. 当 viewModel 中出现新 discard 事件
2. 对应牌从手牌位置飞向弃牌区
3. @react-spring/three: position 从手牌坐标 → 弃牌坐标
4. rotation 同时过渡（竖→平）
5. 动画时长 400ms, ease-out
```

### 副露动画（吃/碰/杠）
```
1. 涉及牌从手牌区滑向副露区
2. 外来牌从弃牌区飞向副露区
3. 动画时长 500ms
```

### 和牌庆祝
```
1. 胡牌涉及的牌依次亮起（emissive 金色）
2. 相机轻微推近（dolly in）
3. 背景水墨晕散粒子效果（使用 Points geometry）
```

### 牌摸入动画
```
1. 摸到新牌时，牌从牌墙位置飞入手牌最右侧
2. 配合 isDrawn 高亮状态
```

---

## 迁移策略（保留逻辑层）

| 文件 | 操作 |
|------|------|
| `App.tsx` | 修改：将 `<BattleScreen>` → `<ThreeBattleScreen>` |
| `BattleScreen.tsx` | 保留不变（可作为 fallback） |
| `src/lib/*` | 完全保留（matchViewModel, tileAssets, socket 等）|
| `src/types/*` | 完全保留 |
| `src/styles/*` | 保留（sidebar/auth 等仍使用 CSS）|
| `TableStage.tsx` | 不使用（由 MahjongScene.tsx 替代）|
| `MahjongTile.tsx` | 不使用（由 MahjongTile3D.tsx 替代）|
| `BottomActionDock.tsx` | 部分复用（抽出按钮逻辑到 ActionDockOverlay）|
| `ResultOverlay.tsx` | 可直接复用（HTML 弹窗，叠加在 canvas 上）|

---

## ThreeBattleScreen.tsx 结构

```tsx
export function ThreeBattleScreen(props: BattleScreenProps) {
  return (
    <div style={{ position: 'relative', width: '100%', height: '100vh' }}>
      {/* 3D 主场景 */}
      <Canvas style={{ position: 'absolute', inset: 0 }} shadows>
        <MahjongScene viewModel={props.viewModel}
          onTileSelect={props.onTileSelect}
          onTileDoubleClick={props.onTileDoubleClick} />
      </Canvas>

      {/* HTML 覆盖层 */}
      <TopBarOverlay {...relevantProps} />
      <PlayerInfoOverlay viewModel={props.viewModel} />
      <ActionDockOverlay
        viewModel={props.viewModel}
        onAction={props.onAction}
        onClaimCandidateSelect={props.onClaimCandidateSelect}
        onClaimCandidateActivate={props.onClaimCandidateActivate} />
      {showResult && <ResultOverlay {...relevantProps} />}
      <TableSidebar {...sidebarProps} />
    </div>
  );
}
```

---

## 验证步骤

1. `npm run dev` — 检查 3D 场景是否正常渲染
2. 进入游戏局面，验证：
   - 4 方玩家手牌/暗牌正确显示于 3D 桌面
   - 弃牌河正确出现在各方桌面区域
   - 副露牌组正确显示
   - 中央指示器显示局数/风向
3. 点击/双击手牌：触发出牌逻辑
4. 吃碰杠胡过按钮：测试各操作正常响应
5. 连局/换局：场景正确重置
6. 胡牌和结算界面正常显示
7. 观战模式正常工作
