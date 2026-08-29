import { useState } from "react";
import type { OverviewAction, ProjectSummary } from "@cinematic/domain";
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
import { CinemaWorkspace } from "../cinema/CinemaWorkspace";

interface ProjectWorkspaceProps {
  project: ProjectSummary;
  onCloseProject: () => void;
}

type PanelView = "overview" | "assets" | "workflows" | "production" | "canon" | "worlds" | "scenes" | "providers" | "cinema" | "diagnostics";

export function ProjectWorkspace({
  project,
  onCloseProject,
}: ProjectWorkspaceProps) {
  const [panelView, setPanelView] = useState<PanelView>("overview");
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
    { view: "providers", label: "Providers" },
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
        {panelView === "scenes" ? <SceneWorkspace projectRootPath={project.rootPath} /> : null}
        {panelView === "providers" ? <ProviderSettings projectRootPath={project.rootPath} /> : null}
        {panelView === "diagnostics" ? <DiagnosticsPanel projectRootPath={project.rootPath} /> : null}
        {panelView === "cinema" ? <CinemaWorkspace projectRootPath={project.rootPath} action={overviewAction} /> : null}
      </section>
    </>
  );
}
