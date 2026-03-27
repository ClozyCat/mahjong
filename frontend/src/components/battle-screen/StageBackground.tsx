export function StageBackground() {
  return (
    <div className="stage-background" aria-hidden="true">
      <div className="stage-background__mesh stage-background__mesh--left" />
      <div className="stage-background__mesh stage-background__mesh--right" />
      <div className="stage-background__grid" />
      <div className="stage-background__grain" />
    </div>
  );
}
