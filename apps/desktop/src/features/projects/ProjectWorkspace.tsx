import { useState } from "react";
import type { ProjectSummary } from "@cinematic/domain";
import { BackButton } from "../../components/BackButton";
import { AssetInspector } from "../assets/AssetInspector";
import { AssetList } from "../assets/AssetList";
import { WorkflowWorkspace } from "../workflows/WorkflowWorkspace";
import { ProductionWorkspace } from "../production/ProductionWorkspace";
import { CanonWorkspace } from "../canon/CanonWorkspace";
import { ProviderSettings } from "../providers/ProviderSettings";
import { ProjectOverview } from "../overview/ProjectOverview";

interface ProjectWorkspaceProps {
  project: ProjectSummary;
  onCloseProject: () => void;
}

type PanelView = "overview" | "assets" | "workflows" | "production" | "canon" | "providers";

export function ProjectWorkspace({
  project,
  onCloseProject,
}: ProjectWorkspaceProps) {
  const [panelView, setPanelView] = useState<PanelView>("overview");
  const [selectedAssetId, setSelectedAssetId] = useState<string | null>(null);
  const [assetRefreshKey, setAssetRefreshKey] = useState(0);

  function handleAssetChanged() {
    setAssetRefreshKey((key) => key + 1);
  }

  function handleAssetsPanelBack() {
    setPanelView("overview");
    setSelectedAssetId(null);
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
      <nav>
        <button
          type="button"
          aria-pressed={panelView === "overview"}
          className={panelView === "overview" ? "nav-button nav-button--active" : "nav-button"}
          onClick={() => { setPanelView("overview"); setSelectedAssetId(null); }}
        >
          Overview
        </button>
        <button
          type="button"
          aria-pressed={panelView === "assets"}
          className={panelView === "assets" ? "nav-button nav-button--active" : "nav-button"}
          onClick={() => setPanelView("assets")}
        >
          Assets
        </button>
        <button
          type="button"
          aria-pressed={panelView === "canon"}
          className={panelView === "canon" ? "nav-button nav-button--active" : "nav-button"}
          onClick={() => {
            setPanelView("canon");
            setSelectedAssetId(null);
          }}
        >
          Canon
        </button>
        <button
          type="button"
          aria-pressed={panelView === "workflows"}
          className={panelView === "workflows" ? "nav-button nav-button--active" : "nav-button"}
          onClick={() => {
            setPanelView("workflows");
            setSelectedAssetId(null);
          }}
        >
          Workflows
        </button>
        <button
          type="button"
          aria-pressed={panelView === "production"}
          className={panelView === "production" ? "nav-button nav-button--active" : "nav-button"}
          onClick={() => {
            setPanelView("production");
            setSelectedAssetId(null);
          }}
        >
          Production
        </button>
        <button
          type="button"
          aria-pressed={panelView === "providers"}
          className={panelView === "providers" ? "nav-button nav-button--active" : "nav-button"}
          onClick={() => { setPanelView("providers"); setSelectedAssetId(null); }}
        >
          Providers
        </button>
      </nav>
      <section aria-label="Project workspace">
        {panelView === "overview" ? (
          <ProjectOverview projectRootPath={project.rootPath} onNavigate={setPanelView} />
        ) : null}
        {panelView === "assets" ? (
          <>
            <AssetList
              projectRootPath={project.rootPath}
              selectedAssetId={selectedAssetId}
              refreshKey={assetRefreshKey}
              onSelectAsset={(assetId) => setSelectedAssetId(assetId)}
              onBack={handleAssetsPanelBack}
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
        {panelView === "canon" ? <CanonWorkspace projectRootPath={project.rootPath} /> : null}
        {panelView === "providers" ? <ProviderSettings projectRootPath={project.rootPath} /> : null}
      </section>
    </>
  );
}
