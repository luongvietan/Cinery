import { useState } from "react";
import type { ProjectSummary } from "@cinematic/domain";
import { BackButton } from "../../components/BackButton";
import { AssetInspector } from "../assets/AssetInspector";
import { AssetList } from "../assets/AssetList";
import { WorkflowWorkspace } from "../workflows/WorkflowWorkspace";

interface ProjectWorkspaceProps {
  project: ProjectSummary;
  onCloseProject: () => void;
}

type PanelView = "none" | "assets" | "workflows";

export function ProjectWorkspace({
  project,
  onCloseProject,
}: ProjectWorkspaceProps) {
  const [panelView, setPanelView] = useState<PanelView>("none");
  const [selectedAssetId, setSelectedAssetId] = useState<string | null>(null);
  const [assetRefreshKey, setAssetRefreshKey] = useState(0);

  function handleAssetChanged() {
    setAssetRefreshKey((key) => key + 1);
  }

  function handleAssetsPanelBack() {
    setPanelView("none");
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
          className={panelView === "assets" ? "nav-button nav-button--active" : "nav-button"}
          onClick={() => setPanelView("assets")}
        >
          Assets
        </button>
        <button
          type="button"
          className={panelView === "workflows" ? "nav-button nav-button--active" : "nav-button"}
          onClick={() => {
            setPanelView("workflows");
            setSelectedAssetId(null);
          }}
        >
          Workflows
        </button>
      </nav>
      <section aria-label="Project workspace">
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
      </section>
    </>
  );
}
