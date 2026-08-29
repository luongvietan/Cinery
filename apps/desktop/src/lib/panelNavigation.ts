import type { PanelView } from "../features/projects/panelView";

export const PANEL_NAVIGATION_EVENT = "cinery:navigate-panel";

export function openPanel(panel: PanelView): void {
  window.dispatchEvent(new CustomEvent(PANEL_NAVIGATION_EVENT, { detail: panel }));
}
