import { useState } from "react";
import type { ProjectSummary } from "@cinematic/domain";
import { AssetInspector } from "../assets/AssetInspector";
import { AssetList } from "../assets/AssetList";
import { ImportAssetVersionButton } from "../assets/ImportAssetVersionButton";

interface ProjectWorkspaceProps {
  project: ProjectSummary;
}

type PanelView = "none" | "assets";

export function ProjectWorkspace({ project }: ProjectWorkspaceProps) {
  const [panelView, setPanelView] = useState<PanelView>("none");
  const [selectedAssetId, setSelectedAssetId] = useState<string | null>(null);
  const [importGeneration, setImportGeneration] = useState(0);

  return (
    <>
      <header>
        <h1>{project.name}</h1>
        <span>{project.rootPath}</span>
      </header>
      <nav>
        <button type="button" onClick={() => setPanelView("assets")}>
          Assets
        </button>
      </nav>
      <section aria-label="Project workspace">
        {panelView === "assets" && (
          <>
            <AssetList
              projectRootPath={project.rootPath}
              onSelectAsset={(assetId) => setSelectedAssetId(assetId)}
            />
            {selectedAssetId && (
              <div>
                <AssetInspector
                  key={`${selectedAssetId}:${importGeneration}`}
                  projectRootPath={project.rootPath}
                  assetId={selectedAssetId}
                />
                {/*
                  Keyed on `selectedAssetId` alone (not `importGeneration`,
                  unlike the inspector above) so it remounts - and its
                  in-flight `importing`/`error` state resets - on every asset
                  switch, but not on every import completion for the
                  currently-selected asset.
                */}
                <ImportAssetVersionButton
                  key={selectedAssetId}
                  projectRootPath={project.rootPath}
                  assetId={selectedAssetId}
                  onImported={() =>
                    setImportGeneration((generation) => generation + 1)
                  }
                />
              </div>
            )}
          </>
        )}
      </section>
    </>
  );
}
