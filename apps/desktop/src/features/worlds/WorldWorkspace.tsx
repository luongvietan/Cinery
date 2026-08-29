import { useState } from "react";
import { WorldList } from "./WorldList";
import { WorldDetail } from "./WorldDetail";
import { CreateWorldButton } from "./CreateWorldButton";

export function WorldWorkspace({
  projectRootPath,
}: {
  projectRootPath: string;
}) {
  const [selectedWorldId, setSelectedWorldId] = useState<string | null>(null);
  const [refreshKey, setRefreshKey] = useState(0);
  const [error, setError] = useState<string | null>(null);

  function handleCreated(worldId: string) {
    setRefreshKey((key) => key + 1);
    setSelectedWorldId(worldId);
    setError(null);
  }

  function handleSelect(worldId: string) {
    setSelectedWorldId(worldId);
    setError(null);
  }

  return (
    <section
      aria-label="Worlds workspace"
      className="canon-workspace"
      style={{ width: "100%" }}
    >
      <div className="canon-workspace-toolbar">
        <div>
          <h2>Worlds</h2>
          <p>
            Production Worlds are stable projections of Canon Locations. Each
            World owns one World Plate asset.
          </p>
        </div>
        <CreateWorldButton
          projectRootPath={projectRootPath}
          onCreated={handleCreated}
        />
      </div>
      {error ? <p role="alert">{error}</p> : null}
      <div className="canon-entity-layout" style={{ marginTop: "var(--space-16)" }}>
        <aside aria-label="Worlds list">
          <WorldList
            projectRootPath={projectRootPath}
            selectedWorldId={selectedWorldId}
            onSelectWorld={handleSelect}
            refreshKey={refreshKey}
          />
        </aside>
        <div className="canon-editor-pane">
          {selectedWorldId ? (
            <WorldDetail
              key={selectedWorldId}
              projectRootPath={projectRootPath}
              worldId={selectedWorldId}
              onBack={() => setSelectedWorldId(null)}
            />
          ) : (
            <p>Select a world to see its backdrop and generate its scenery.</p>
          )}
        </div>
      </div>
    </section>
  );
}
