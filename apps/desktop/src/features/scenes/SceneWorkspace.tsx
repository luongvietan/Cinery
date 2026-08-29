import { useState } from "react";
import { SceneList } from "./SceneList";
import { SceneEditor } from "./SceneEditor";
import { SceneWorldAssignment } from "./SceneWorldAssignment";
import { SceneCharacterAssignments } from "./SceneCharacterAssignments";
import { ScenePropAssignments } from "./ScenePropAssignments";
import { SceneTbdPanel } from "./SceneTbdPanel";
import { SceneReadinessPanel } from "./SceneReadinessPanel";
import { SceneShots } from "./SceneShots";
import { SceneCompile } from "./SceneCompile";
import { createScene } from "./api";
import { describeError } from "../../lib/errors";
import { useEffect, useRef } from "react";

export function SceneWorkspace({
  projectRootPath,
  initialSceneId = null,
}: {
  projectRootPath: string;
  initialSceneId?: string | null;
}) {
  const [selectedSceneId, setSelectedSceneId] = useState<string | null>(initialSceneId);
  const [refreshKey, setRefreshKey] = useState(0);
  const [isCreateOpen, setIsCreateOpen] = useState(false);
  const [newTitle, setNewTitle] = useState("");
  const [newSummary, setNewSummary] = useState("");
  const [creating, setCreating] = useState(false);
  const [createError, setCreateError] = useState<string | null>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const titleInputRef = useRef<HTMLInputElement>(null);

  // focus management
  useEffect(() => {
    if (isCreateOpen) {
      const id = requestAnimationFrame(() => titleInputRef.current?.focus());
      return () => cancelAnimationFrame(id);
    } else {
      requestAnimationFrame(() => triggerRef.current?.focus());
    }
  }, [isCreateOpen]);

  useEffect(() => {
    if (!isCreateOpen) return;
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") setIsCreateOpen(false);
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [isCreateOpen]);

  function handleCreated(sceneId: string) {
    setRefreshKey((k) => k + 1);
    setSelectedSceneId(sceneId);
    setIsCreateOpen(false);
    setNewTitle("");
    setNewSummary("");
    setCreateError(null);
  }

  function handleSelect(sceneId: string) {
    setSelectedSceneId(sceneId);
    setCreateError(null);
  }

  async function handleCreate() {
    const trimmedTitle = newTitle.trim();
    if (!trimmedTitle) {
      setCreateError("Title must not be empty");
      return;
    }
    setCreating(true);
    setCreateError(null);
    try {
      const created = await createScene(projectRootPath, trimmedTitle, newSummary);
      handleCreated(created.id);
    } catch (caught: unknown) {
      setCreateError(describeError(caught));
    } finally {
      setCreating(false);
    }
  }

  return (
    <section aria-label="Scenes workspace" className="canon-workspace" style={{ width: "100%" }}>
      <div className="canon-workspace-toolbar">
        <div>
          <h2>Scenes</h2>
          <p>Assemble exact immutable visual references — World, Characters, Props — without copying asset IDs manually.</p>
        </div>
        <button type="button" ref={triggerRef} onClick={() => setIsCreateOpen(true)}>
          New Scene
        </button>
      </div>

      {createError ? <p role="alert">{createError}</p> : null}

      <div className="canon-entity-layout" style={{ marginTop: "var(--space-16)" }}>
        <aside aria-label="Scenes list">
          <SceneList
            projectRootPath={projectRootPath}
            selectedSceneId={selectedSceneId}
            onSelectScene={handleSelect}
            refreshKey={refreshKey}
          />
        </aside>
        <div className="canon-editor-pane" style={{ display: "flex", flexDirection: "column", gap: "var(--space-16)" }}>
          {selectedSceneId ? (
            <>
              <SceneEditor
                key={`editor-${selectedSceneId}-${refreshKey}`}
                projectRootPath={projectRootPath}
                sceneId={selectedSceneId}
                onUpdated={() => setRefreshKey((k) => k + 1)}
                onBack={() => setSelectedSceneId(null)}
              />
              <SceneWorldAssignment
                key={`world-${selectedSceneId}-${refreshKey}`}
                projectRootPath={projectRootPath}
                sceneId={selectedSceneId}
                onChanged={() => setRefreshKey((k) => k + 1)}
              />
              <SceneCharacterAssignments
                key={`chars-${selectedSceneId}-${refreshKey}`}
                projectRootPath={projectRootPath}
                sceneId={selectedSceneId}
                onChanged={() => setRefreshKey((k) => k + 1)}
              />
              <ScenePropAssignments
                key={`props-${selectedSceneId}-${refreshKey}`}
                projectRootPath={projectRootPath}
                sceneId={selectedSceneId}
                onChanged={() => setRefreshKey((k) => k + 1)}
              />
              <SceneTbdPanel projectRootPath={projectRootPath} sceneId={selectedSceneId} />
              <SceneReadinessPanel projectRootPath={projectRootPath} sceneId={selectedSceneId} refreshKey={refreshKey} />
              <SceneShots
                key={`shots-${selectedSceneId}-${refreshKey}`}
                projectRootPath={projectRootPath}
                sceneId={selectedSceneId}
                onChanged={() => setRefreshKey((k) => k + 1)}
              />
              <SceneCompile
                key={`compile-${selectedSceneId}-${refreshKey}`}
                projectRootPath={projectRootPath}
                sceneId={selectedSceneId}
                onChanged={() => setRefreshKey((k) => k + 1)}
              />
            </>
          ) : (
            <p>Select a scene to assemble its World, Characters, and Props — or create a new Scene.</p>
          )}
        </div>
      </div>

      {isCreateOpen ? (
        <div className="canon-dialog-backdrop" role="presentation" onClick={() => setIsCreateOpen(false)}>
          <div
            role="dialog"
            aria-modal="true"
            aria-labelledby="create-scene-title"
            className="canon-dialog"
            onClick={(event) => event.stopPropagation()}
          >
            <header>
              <h2 id="create-scene-title">Create Scene</h2>
              <button type="button" className="canon-secondary-button" onClick={() => setIsCreateOpen(false)} aria-label="Close">
                ✕
              </button>
            </header>
            <p>Scenes assemble exact immutable references. New scenes are numbered sequentially (SCENE-001, SCENE-002…) and persist across restarts.</p>
            {createError ? <p role="alert">{createError}</p> : null}
            <div className="canon-field-grid" style={{ marginTop: "var(--space-12)" }}>
              <label htmlFor="new-scene-title">
                Title
                <input
                  id="new-scene-title"
                  ref={titleInputRef}
                  value={newTitle}
                  onChange={(event) => setNewTitle(event.target.value)}
                  placeholder="e.g. Night Transmission"
                />
              </label>
              <label htmlFor="new-scene-summary">
                Summary
                <textarea
                  id="new-scene-summary"
                  value={newSummary}
                  onChange={(event) => setNewSummary(event.target.value)}
                  placeholder="Brief summary of the scene…"
                  rows={3}
                  style={{
                    width: "100%",
                    padding: "var(--space-8) var(--space-12)",
                    border: "1px solid var(--c-hairline)",
                    borderRadius: "var(--radius-md)",
                    fontFamily: "inherit",
                  }}
                />
              </label>
            </div>
            <div style={{ display: "flex", gap: "var(--space-8)", marginTop: "var(--space-16)" }}>
              <button type="button" onClick={() => void handleCreate()} disabled={creating || !newTitle.trim()}>
                {creating ? "Creating…" : "Create Scene"}
              </button>
              <button type="button" className="canon-secondary-button" onClick={() => setIsCreateOpen(false)} disabled={creating}>
                Cancel
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </section>
  );
}
