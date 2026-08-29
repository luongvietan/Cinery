import { useEffect, useState } from "react";
import type { OverviewAction, ProjectSummary } from "@cinematic/domain";
import { PANEL_NAVIGATION_EVENT } from "../../lib/panelNavigation";
import type { PanelView } from "./panelView";
import { BackButton } from "../../components/BackButton";
import { GooeyNav, GooeyNavItem } from "../../components/GooeyNav";
import { AssetInspector } from "../assets/AssetInspector";
import { AssetList } from "../assets/AssetList";
import { WorkflowWorkspace } from "../workflows/WorkflowWorkspace";
import { ProductionWorkspace } from "../production/ProductionWorkspace";
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

export function ProjectWorkspace({
  project,
  onCloseProject,
}: ProjectWorkspaceProps) {
  const [panelView, setPanelView] = useState<PanelView>("overview");

  useEffect(() => {
    function handlePanelNavigation(event: Event) {
      const detail = (event as CustomEvent<PanelView>).detail;
      if (typeof detail === "string" && detail) {
        setPanelView(detail as PanelView);
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
    setOverviewAction(action);
    setPanelView(action.destination);
    setSelectedAssetId(null);
  }

  const navItems: Array<{ view: PanelView; label: string }> = [
    { view: "overview", label: "Overview" },
    { view: "assets", label: "Assets" },
    { view: "canon", label: "Canon" },
    { view: "workflows", label: "Workflows" },
    { view: "production", label: "Production" },
    { view: "worlds", label: "Worlds" },
    { view: "scenes", label: "Scenes" },
    { view: "providers", label: "AI Services" },
    { view: "diagnostics", label: "Diagnostics" },
  ];

  return (
    <>
      <header className="workspace-header">
        <BackButton label="← Projects" onClick={onCloseProject} />
        <div className="workspace-header-copy">
          <h1>{project.name}</h1>
          <span>{project.rootPath}</span>
        </div>
      </header>
      <GooeyNav ariaLabel="Workspace panels">
        {navItems.map(({ view, label }) => (
          <GooeyNavItem
            key={view}
            label={label}
            pressed={panelView === view}
            className={panelView === view ? "nav-button nav-button--active" : "nav-button"}
            onClick={() => {
              setPanelView(view);
              setSelectedAssetId(null);
            }}
          />
        ))}
      </GooeyNav>
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
        {panelView === "production" ? (
          <ProductionWorkspace projectRootPath={project.rootPath} />
        ) : null}
        {panelView === "canon" ? <CanonWorkspace projectRootPath={project.rootPath} initialTab={overviewAction?.id === "resolve_protected_tbd" ? "TBDs" : "Story"} /> : null}
        {panelView === "worlds" ? <WorldWorkspace projectRootPath={project.rootPath} /> : null}
        {panelView === "scenes" ? <SceneWorkspace projectRootPath={project.rootPath} initialSceneId={overviewAction?.destination === "scenes" ? overviewAction.sceneId : null} /> : null}
        {panelView === "providers" ? <ProviderSettings projectRootPath={project.rootPath} /> : null}
        {panelView === "diagnostics" ? <DiagnosticsPanel projectRootPath={project.rootPath} /> : null}
      </section>
    </>
  );
}
