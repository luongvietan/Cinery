import { useEffect, useState } from "react";
import type { OverviewAction, ProjectSummary } from "@cinematic/domain";
import { PANEL_NAVIGATION_EVENT, type PanelTarget } from "../../lib/panelNavigation";
import type { PanelView } from "./panelView";
import { BackButton } from "../../components/BackButton";
import { GooeyNav, GooeyNavItem } from "../../components/GooeyNav";
import { AssetInspector } from "../assets/AssetInspector";
import { AssetList } from "../assets/AssetList";
import { WorkflowWorkspace } from "../workflows/WorkflowWorkspace";
import { CanonWorkspace } from "../canon/CanonWorkspace";
import { WorldWorkspace } from "../worlds/WorldWorkspace";
import { SceneWorkspace } from "../scenes/SceneWorkspace";
import { ProviderSettings } from "../providers/ProviderSettings";
import { DiagnosticsPanel } from "../diagnostics/DiagnosticsPanel";
import { ProjectOverview } from "../overview/ProjectOverview";

interface ProjectWorkspaceProps {
  project: ProjectSummary;
  onCloseProject: () => void;
}

/**
 * Navigation is grouped by intent instead of one flat row: the creative
 * journey (Overview → Story → Worlds → Scenes → Assets), the output it
 * produces (Generations), and rarely-needed system surfaces (AI Services,
 * Support) are visually separated so configuration never competes with
 * creation.
 */
const CREATE_NAV: Array<{ view: PanelView; label: string }> = [
  { view: "overview", label: "Overview" },
  { view: "canon", label: "Story" },
  { view: "worlds", label: "Worlds" },
  { view: "scenes", label: "Sequences" },
  { view: "assets", label: "Assets" },
];
const OUTPUT_NAV: Array<{ view: PanelView; label: string }> = [
  { view: "workflows", label: "Generations" },
];
const SYSTEM_NAV: Array<{ view: PanelView; label: string }> = [
  { view: "providers", label: "AI Services" },
  { view: "diagnostics", label: "Support" },
];

/** Backend readiness actions still address the removed Production panel; route
 * character work to Story → Characters with the character preselected. */
function destinationPanel(action: OverviewAction): { view: PanelView; canonTab?: "Characters" | "TBDs" } {
  if (action.destination === "production") return { view: "canon", canonTab: "Characters" };
  if (action.destination === "canon" && action.id === "resolve_protected_tbd") return { view: "canon", canonTab: "TBDs" };
  return { view: action.destination };
}

/** Scoped scene actions land on the tab where that work happens, so the
 * user never has to find the right section after clicking a CTA. */
function sceneTabFor(action: OverviewAction): "Setup" | "Shots" | "Render" {
  if (action.id === "cinema_compilation") return "Render";
  if (action.id === "scene" || action.id === "scene_readiness") return "Setup";
  return "Shots";
}

export function ProjectWorkspace({
  project,
  onCloseProject,
}: ProjectWorkspaceProps) {
  const [panelView, setPanelView] = useState<PanelView>("overview");
  const [canonTab, setCanonTab] = useState<"Characters" | "TBDs" | null>(null);
  const [focusCharacterId, setFocusCharacterId] = useState<string | null>(null);
  const [sceneTab, setSceneTab] = useState<"Setup" | "Shots" | "Render" | null>(null);

  useEffect(() => {
    function handlePanelNavigation(event: Event) {
      const detail = (event as CustomEvent<PanelView | PanelTarget>).detail;
      if (typeof detail === "string" && detail) {
        setPanelView(detail as PanelView);
        setSelectedAssetId(null);
      } else if (detail && typeof detail === "object" && "panel" in detail) {
        setPanelView(detail.panel);
        setCanonTab(detail.canonTab ?? null);
        setFocusCharacterId(null);
        setSelectedAssetId(null);
      }
    }
    window.addEventListener(PANEL_NAVIGATION_EVENT, handlePanelNavigation);
    return () => window.removeEventListener(PANEL_NAVIGATION_EVENT, handlePanelNavigation);
  }, []);
  const [selectedAssetId, setSelectedAssetId] = useState<string | null>(null);
  const [assetRefreshKey, setAssetRefreshKey] = useState(0);
  const [overviewAction, setOverviewAction] = useState<OverviewAction | null>(null);

  function handleAssetChanged() {
    setAssetRefreshKey((key) => key + 1);
  }

  function handleAssetsPanelBack() {
    setPanelView("overview");
    setSelectedAssetId(null);
  }

  function navigateOverviewAction(action: OverviewAction) {
    const target = destinationPanel(action);
    setOverviewAction(action);
    setPanelView(target.view);
    setCanonTab(target.canonTab ?? null);
    setFocusCharacterId(target.canonTab === "Characters" ? action.characterEntityId : null);
    setSceneTab(target.view === "scenes" ? sceneTabFor(action) : null);
    setSelectedAssetId(null);
  }

  function openPanel(view: PanelView) {
    setPanelView(view);
    setSelectedAssetId(null);
    setCanonTab(null);
    setFocusCharacterId(null);
    setSceneTab(null);
  }

  function renderNav(
    items: Array<{ view: PanelView; label: string }>,
    ariaLabel: string,
    keyPrefix: string,
  ) {
    return (
      <GooeyNav ariaLabel={ariaLabel} key={keyPrefix}>
        {items.map(({ view, label }) => (
          <GooeyNavItem
            key={view}
            label={label}
            pressed={panelView === view}
            className={panelView === view ? "nav-button nav-button--active" : "nav-button"}
            onClick={() => openPanel(view)}
          />
        ))}
      </GooeyNav>
    );
  }

  return (
    <>
      <header className="workspace-header">
        <BackButton label="← Projects" onClick={onCloseProject} />
        <div className="workspace-header-copy">
          <h1>{project.name}</h1>
          <span>{project.rootPath}</span>
        </div>
      </header>
      <nav className="workspace-nav" aria-label="Workspace areas">
        {renderNav(CREATE_NAV, "Create", "create")}
        <span className="workspace-nav__divider" aria-hidden="true" />
        {renderNav(OUTPUT_NAV, "Output", "output")}
        <span className="workspace-nav__divider" aria-hidden="true" />
        {renderNav(SYSTEM_NAV, "System", "system")}
      </nav>
      <section aria-label="Project workspace">
        {panelView === "overview" ? (
          <ProjectOverview projectRootPath={project.rootPath} onNavigate={navigateOverviewAction} />
        ) : null}
        {panelView === "assets" ? (
          <>
            <AssetList
              projectRootPath={project.rootPath}
              selectedAssetId={selectedAssetId}
              refreshKey={assetRefreshKey}
              onSelectAsset={(assetId) => setSelectedAssetId(assetId)}
              onBack={handleAssetsPanelBack}
              defaultOwnerEntityId={overviewAction?.destination === "assets" ? overviewAction.characterEntityId : null}
            />
            {selectedAssetId ? (
              <AssetInspector
                key={selectedAssetId}
                projectRootPath={project.rootPath}
                assetId={selectedAssetId}
                onAssetChanged={handleAssetChanged}
                onBack={() => setSelectedAssetId(null)}
              />
            ) : null}
          </>
        ) : null}
        {panelView === "workflows" ? (
          <WorkflowWorkspace projectRootPath={project.rootPath} />
        ) : null}
        {panelView === "canon" ? (
          <CanonWorkspace
            projectRootPath={project.rootPath}
            initialTab={canonTab ?? undefined}
            initialCharacterId={focusCharacterId}
          />
        ) : null}
        {panelView === "worlds" ? <WorldWorkspace projectRootPath={project.rootPath} /> : null}
        {panelView === "scenes" ? <SceneWorkspace projectRootPath={project.rootPath} initialSceneId={overviewAction?.destination === "scenes" ? overviewAction.sceneId : null} initialTab={sceneTab ?? undefined} /> : null}
        {panelView === "providers" ? <ProviderSettings projectRootPath={project.rootPath} /> : null}
        {panelView === "diagnostics" ? <DiagnosticsPanel projectRootPath={project.rootPath} /> : null}
      </section>
    </>
  );
}
