import type { PanelView } from "../features/projects/panelView";

export const PANEL_NAVIGATION_EVENT = "cinery:navigate-panel";

/** Optional per-panel context, e.g. which Story tab to open. */
export interface PanelTarget {
  panel: PanelView;
  canonTab?: "Characters" | "TBDs";
}

export function openPanel(panel: PanelView): void {
  window.dispatchEvent(new CustomEvent(PANEL_NAVIGATION_EVENT, { detail: panel }));
}

export function openPanelTarget(target: PanelTarget): void {
  window.dispatchEvent(new CustomEvent(PANEL_NAVIGATION_EVENT, { detail: target }));
}
