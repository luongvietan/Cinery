import { useEffect, useRef, useState } from "react";
import { SceneList } from "./SceneList";
import { SceneEditor } from "./SceneEditor";
import { SceneWorldAssignment } from "./SceneWorldAssignment";
import { SceneCharacterAssignments } from "./SceneCharacterAssignments";
import { ScenePropAssignments } from "./ScenePropAssignments";
import { SceneTbdPanel } from "./SceneTbdPanel";
import { SceneReadinessPanel } from "./SceneReadinessPanel";
import { SceneShots } from "./SceneShots";
import { SceneCompile } from "./SceneCompile";
import { SequenceBrief } from "./SequenceBrief";
import { createScene, getCompileReadiness } from "./api";
import { getSequenceFlow } from "./sequenceFlowApi";
import { describeError } from "../../lib/errors";
import type { SequenceFlow } from "@cinematic/domain";

type SceneTab = "Setup" | "Shots" | "Render";

interface SceneWorkspaceProps {
  projectRootPath: string;
  initialSceneId?: string | null;
  /** Deep-links the editor tab when arriving from a scoped action
   * (e.g. Overview's "Compile the final prompt" opens Render directly). */
  initialTab?: SceneTab;
}

export function SceneWorkspace({
  projectRootPath,
  initialSceneId = null,
  initialTab,
}: SceneWorkspaceProps) {
  const [selectedSceneId, setSelectedSceneId] = useState<string | null>(initialSceneId);
  const [refreshKey, setRefreshKey] = useState(0);
  const [tab, setTab] = useState<SceneTab>(initialTab ?? "Setup");
  const [readiness, setReadiness] = useState<{ ready: boolean; blockers: Array<{ message: string }> } | null>(null);
  const [flow, setFlow] = useState<SequenceFlow | null>(null);
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

  // Scene readiness drives the header status and the dominant Render CTA.
  useEffect(() => {
    if (!selectedSceneId) { setReadiness(null); return; }
    let cancelled = false;
    getCompileReadiness(projectRootPath, selectedSceneId)
      .then((next) => { if (!cancelled) setReadiness(next); })
      .catch(() => { if (!cancelled) setReadiness(null); });
    return () => { cancelled = true; };
  }, [projectRootPath, selectedSceneId, refreshKey]);

  // A scene without a saved flow yet simply has none (SEQUENCE_FLOW_NOT_FOUND
  // is the normal fresh-scene case, surfaced here as null).
  useEffect(() => {
    if (!selectedSceneId) { setFlow(null); return; }
    let cancelled = false;
    getSequenceFlow(projectRootPath, selectedSceneId)
      .then((next) => { if (!cancelled) setFlow(next); })
      .catch(() => { if (!cancelled) setFlow(null); });
    return () => { cancelled = true; };
  }, [projectRootPath, selectedSceneId, refreshKey]);

  function handleCreated(sceneId: string) {
    setRefreshKey((k) => k + 1);
    setSelectedSceneId(sceneId);
    setTab("Setup");
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

  function handleChanged() {
    setRefreshKey((k) => k + 1);
  }

  const blockers = readiness?.blockers ?? [];

  return (
    <section aria-label="Scenes workspace" className="canon-workspace" style={{ width: "100%" }}>
      <div className="canon-workspace-toolbar">
        <div>
          <h2>Scenes</h2>
          <p>Stage each scene in its world with your characters, then generate its keyframes and video.</p>
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
              <div className="scene-header" role="group" aria-label="Scene header">
                <div className="scene-header__tabs" role="tablist" aria-label="Scene editor sections">
                  {(["Setup", "Shots", "Render"] as const).map((item) => (
                    <button
                      type="button"
                      role="tab"
                      key={item}
                      id={`scene-tab-${item.toLowerCase()}`}
                      aria-selected={tab === item}
                      aria-controls={`scene-panel-${item.toLowerCase()}`}
                      className={tab === item ? "nav-button nav-button--active scene-header__tab" : "nav-button scene-header__tab"}
                      onClick={() => setTab(item)}
                    >
                      {item}
                    </button>
                  ))}
                </div>
                <div className="scene-header__status">
                  {blockers.length ? (
                    <span className="scene-status-chip scene-status-chip--blocked" title={blockers.map((blocker) => blocker.message).join(" ")}>
                      <span aria-hidden="true">!</span> {blockers.length} to fix
                    </span>
                  ) : (
                    <span className="scene-status-chip scene-status-chip--ready"><span aria-hidden="true">✓</span> Ready to render</span>
                  )}
                  {tab !== "Render" ? (
                    <button type="button" className="scene-header__cta" onClick={() => setTab("Render")}>
                      Go to Render
                    </button>
                  ) : null}
                </div>
              </div>
              <div
                role="tabpanel"
                id={`scene-panel-${tab.toLowerCase()}`}
                aria-labelledby={`scene-tab-${tab.toLowerCase()}`}
                style={{ display: "flex", flexDirection: "column", gap: "var(--space-16)" }}
              >
                {tab === "Setup" ? (
                  <>
                    <SequenceBrief
                      projectRootPath={projectRootPath}
                      sceneId={selectedSceneId}
                      flow={flow}
                      onChanged={handleChanged}
                    />
                    <SceneEditor
                      key={`editor-${selectedSceneId}-${refreshKey}`}
                      projectRootPath={projectRootPath}
                      sceneId={selectedSceneId}
                      onUpdated={handleChanged}
                      onBack={() => setSelectedSceneId(null)}
                    />
                    <SceneWorldAssignment
                      key={`world-${selectedSceneId}-${refreshKey}`}
                      projectRootPath={projectRootPath}
                      sceneId={selectedSceneId}
                      onChanged={handleChanged}
                    />
                    <SceneCharacterAssignments
                      key={`chars-${selectedSceneId}-${refreshKey}`}
                      projectRootPath={projectRootPath}
                      sceneId={selectedSceneId}
                      onChanged={handleChanged}
                    />
                    <ScenePropAssignments
                      key={`props-${selectedSceneId}-${refreshKey}`}
                      projectRootPath={projectRootPath}
                      sceneId={selectedSceneId}
                      onChanged={handleChanged}
                    />
                    <SceneTbdPanel
                      key={`tbd-${selectedSceneId}-${refreshKey}`}
                      projectRootPath={projectRootPath}
                      sceneId={selectedSceneId}
                      onDecisionsChanged={handleChanged}
                    />
                    <SceneReadinessPanel projectRootPath={projectRootPath} sceneId={selectedSceneId} refreshKey={refreshKey} />
                  </>
                ) : null}
                {tab === "Shots" ? (
                  <SceneShots
                    key={`shots-${selectedSceneId}-${refreshKey}`}
                    projectRootPath={projectRootPath}
                    sceneId={selectedSceneId}
                    onChanged={handleChanged}
                  />
                ) : null}
                {tab === "Render" ? (
                  <SceneCompile
                    key={`compile-${selectedSceneId}-${refreshKey}`}
                    projectRootPath={projectRootPath}
                    sceneId={selectedSceneId}
                    onChanged={handleChanged}
                  />
                ) : null}
              </div>
            </>
          ) : (
            <div className="empty-state" role="status">
              <p>No scene selected</p>
              <p>Pick a scene on the left to stage it, or create a new one. A scene pulls in your world, cast, and props, and breaks down into shots you can render.</p>
            </div>
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
              <h2 id="create-scene-title">New Scene</h2>
              <button type="button" className="canon-secondary-button" onClick={() => setIsCreateOpen(false)} aria-label="Close">
                ✕
              </button>
            </header>
            <p>Scenes are numbered automatically (SCENE-001, SCENE-002…) and stay on your computer.</p>
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
                  placeholder="What happens in this scene…"
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
