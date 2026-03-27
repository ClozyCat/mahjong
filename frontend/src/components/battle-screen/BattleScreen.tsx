import type { BattleActionId, BattleViewModel } from '../../types/match';
import { AmbientOverlay } from './AmbientOverlay';
import { BottomActionDock } from './BottomActionDock';
import { PlayerRing } from './PlayerRing';
import { ResultOverlay } from './ResultOverlay';
import { StageBackground } from './StageBackground';
import { TableStage } from './TableStage';
import { TopMatchBar } from './TopMatchBar';
import { WindowFrame } from '../win98/WindowFrame';

interface BattleScreenProps {
  viewModel: BattleViewModel;
  onTileSelect: (tileId: string) => void;
  onAction: (actionId: BattleActionId) => void;
  onCopyTableCode: () => void;
}

const REMOTE_PLAYER_ORDER = ['left', 'top', 'right'] as const;

export function BattleScreen({ viewModel, onTileSelect, onAction, onCopyTableCode }: BattleScreenProps) {
  const localPlayer = viewModel.players.find((item) => item.isLocal) ?? null;
  const remotePlayers = REMOTE_PLAYER_ORDER.map((seat) => viewModel.players.find((item) => item.seat === seat)).filter(
    (player): player is NonNullable<typeof player> => Boolean(player),
  );

  return (
    <main className="battle-screen">
      <StageBackground />
      <WindowFrame title="四风麻将客户端" status={`状态：${viewModel.topStatusLabel}`} className="battle-shell">
        <TopMatchBar
          tableCode={viewModel.tableCode}
          phaseLabel={viewModel.phaseLabel}
          roundLabel={viewModel.roundLabel}
          scoreSummaryLabel={viewModel.scoreSummaryLabel}
          deadlineAt={viewModel.deadlineAt}
          topStatusLabel={viewModel.topStatusLabel}
          onCopyTableCode={onCopyTableCode}
        />
        <div className="battle-stage">
          <div className="battle-stage__halo" />
          <div className="battle-stage__player-row">
            {remotePlayers.map((player) => (
              <PlayerRing key={player.seat} player={player} />
            ))}
          </div>
          <div className="battle-stage__table-wrap">
            <TableStage
              discards={viewModel.discards}
              activeSeat={viewModel.activePlayerSeat}
              lastDiscard={viewModel.lastDiscard}
              lastDiscardSeat={viewModel.lastDiscardSeat}
              promptText={viewModel.promptText}
              players={viewModel.players}
            />
          </div>
          <AmbientOverlay
            mode={viewModel.mode}
            banner={viewModel.centerBanner}
            promptText={viewModel.promptText}
            waitingControls={viewModel.waitingControls}
            toasts={viewModel.toasts}
          />
          {viewModel.result ? <ResultOverlay result={viewModel.result} onAction={onAction} /> : null}
        </div>
        <BottomActionDock
          hand={viewModel.localHand}
          actions={viewModel.actions}
          isElevated={viewModel.isActionDockElevated}
          waitingControls={viewModel.waitingControls}
          localPlayer={localPlayer}
          onTileSelect={onTileSelect}
          onAction={onAction}
        />
      </WindowFrame>
    </main>
  );
}
