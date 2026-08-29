// Panel identifiers are internal routing ids and stay stable across UI
// renames: the nav shows "Story"/"Generations"/"Support" while the ids below
// keep working for deep links, tests, and persisted navigation events.
// "production" was folded into Story → Characters; its readiness actions are
// retargeted at the shell (see ProjectWorkspace).
export type PanelView =
  | "overview"
  | "assets"
  | "canon"
  | "workflows"
  | "worlds"
  | "scenes"
  | "providers"
  | "diagnostics";
