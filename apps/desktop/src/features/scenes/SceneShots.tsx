import { useEffect, useState } from "react";
import type { WorkflowRunDetail } from "@cinematic/domain";
import { describeError } from "../../lib/errors";
import {
  createShot,
  deleteShot,
  ensureSceneKeyframeAsset,
  listShots,
  reorderShots,
  setShotKeyframe,
  updateShot,
  type Shot,
} from "./api";
import { advanceWorkflowRun, createWorkflowRun, listSkillOperations } from "../workflows/api";
import { listAssets } from "../assets/api";
import { deriveGenerationResultContext, type AssetSummary } from "@cinematic/domain";
import { GenerationResults } from "../generation/GenerationResults";
import { listGenerationResults } from "../generation/api";
import { WorkflowRunView } from "../workflows/WorkflowRunView";

interface SceneShotsProps {
  projectRootPath: string;
  sceneId: string;
  onChanged?: () => void;
}

interface KeyframeRunState {
  shotId: string;
  detail: WorkflowRunDetail;
  context: ReturnType<typeof deriveGenerationResultContext>;
}

/**
 * Shot list for the authoritative Scene: create, edit, delete, reorder, and
 * the Shot → Keyframe flow — generate, review every candidate in a grid,
 * then explicitly "Use this keyframe" to pin the chosen version.
 */
export function SceneShots({ projectRootPath, sceneId, onChanged }: SceneShotsProps) {
  const [shots, setShots] = useState<Shot[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);

  const [adding, setAdding] = useState(false);
  const [intent, setIntent] = useState("");
  const [duration, setDuration] = useState("4");
  const [action, setAction] = useState("");
  const [camera, setCamera] = useState("");

  const [editingId, setEditingId] = useState<string | null>(null);
  const [editIntent, setEditIntent] = useState("");
  const [editDuration, setEditDuration] = useState("");
  const [editAction, setEditAction] = useState("");
  const [editCamera, setEditCamera] = useState("");

  const [keyframeRun, setKeyframeRun] = useState<KeyframeRunState | null>(null);
  const [pinningShotId, setPinningShotId] = useState<string | null>(null);
  const [assets, setAssets] = useState<AssetSummary[]>([]);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    setKeyframeRun(null);
    listShots(projectRootPath, sceneId)
      .then((next) => {
        if (!cancelled) setShots(next);
      })
      .catch((caught: unknown) => {
        if (!cancelled) setError(describeError(caught));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [projectRootPath, sceneId]);

  function refresh() {
    return listShots(projectRootPath, sceneId)
      .then(setShots)
      .catch((caught: unknown) => setActionError(describeError(caught)));
  }

  function notifyChanged() {
    onChanged?.();
  }

  async function handleAdd() {
    const trimmed = intent.trim();
    if (!trimmed) {
      setActionError("Shot intent is required");
      return;
    }
    const seconds = Number(duration);
    if (!Number.isFinite(seconds) || seconds <= 0 || seconds > 30) {
      setActionError("Duration must be between 0.5 and 30 seconds");
      return;
    }
    setPending(true);
    setActionError(null);
    try {
      await createShot(projectRootPath, sceneId, seconds, trimmed, action.trim() || null, camera.trim() || null);
      setIntent("");
      setDuration("4");
      setAction("");
      setCamera("");
      setAdding(false);
      await refresh();
      notifyChanged();
    } catch (caught: unknown) {
      setActionError(describeError(caught));
    } finally {
      setPending(false);
    }
  }

  function beginEdit(shot: Shot) {
    setEditingId(shot.id);
    setEditIntent(shot.intent);
    setEditDuration(String(shot.durationSeconds));
    setEditAction(shot.action ?? "");
    setEditCamera(shot.camera ?? "");
  }

  async function handleSaveEdit(shotId: string) {
    const seconds = Number(editDuration);
    if (!Number.isFinite(seconds) || seconds <= 0 || seconds > 30) {
      setActionError("Duration must be between 0.5 and 30 seconds");
      return;
    }
    if (!editIntent.trim()) {
      setActionError("Shot intent is required");
      return;
    }
    setPending(true);
    setActionError(null);
    try {
      await updateShot(projectRootPath, shotId, {
        durationSeconds: seconds,
        intent: editIntent.trim(),
        action: editAction.trim() || null,
        camera: editCamera.trim() || null,
      });
      setEditingId(null);
      await refresh();
      notifyChanged();
    } catch (caught: unknown) {
      setActionError(describeError(caught));
    } finally {
      setPending(false);
    }
  }

  async function handleDelete(shotId: string) {
    if (!window.confirm("Delete this shot? Its keyframe reference is cleared; assets are kept.")) {
      return;
    }
    setPending(true);
    setActionError(null);
    try {
      await deleteShot(projectRootPath, sceneId, shotId);
      if (editingId === shotId) setEditingId(null);
      await refresh();
      notifyChanged();
    } catch (caught: unknown) {
      setActionError(describeError(caught));
    } finally {
      setPending(false);
    }
  }

  async function handleMove(shotId: string, direction: -1 | 1) {
    const index = shots.findIndex((shot) => shot.id === shotId);
    const target = index + direction;
    if (index < 0 || target < 0 || target >= shots.length) return;
    const ordered = [...shots];
    const [moved] = ordered.splice(index, 1);
    ordered.splice(target, 0, moved);
    setPending(true);
    setActionError(null);
    try {
      const next = await reorderShots(projectRootPath, sceneId, ordered.map((shot) => shot.id));
      setShots(next);
      notifyChanged();
    } catch (caught: unknown) {
      setActionError(describeError(caught));
    } finally {
      setPending(false);
    }
  }

  async function handleGenerateKeyframe(shotId: string) {
    setPending(true);
    setActionError(null);
    try {
      // Ensure the scene's stable keyframe asset exists before generating.
      await ensureSceneKeyframeAsset(projectRootPath, sceneId);
      // No provider is hardcoded: the backend uses the project's configured
      // default AI service, or fails with guidance if none is connected.
      const created = await createWorkflowRun(projectRootPath, "scene-builder", "1.0.0", "scene.create_keyframe", {
        sceneId,
      });
      const waiting = await advanceWorkflowRun(projectRootPath, created.run.id);
      await applyRun(shotId, waiting);
    } catch (caught: unknown) {
      setActionError(describeError(caught));
    } finally {
      setPending(false);
    }
  }

  /** Resolve the run review state: fetch candidates once a run completes so
   * the user always picks from the actual images before pinning. */
  async function applyRun(shotId: string, detail: WorkflowRunDetail) {
    let context: KeyframeRunState["context"] = null;
    let assetSummaries = assets;
    if (detail.run.status === "completed") {
      try {
        const [operations, assetList, resultSets] = await Promise.all([
          listSkillOperations(),
          listAssets(projectRootPath),
          listGenerationResults(projectRootPath, detail.run.id),
        ]);
        const operation = operations.find((candidate) => candidate.id === detail.run.operationId) ?? null;
        if (operation) {
          const derived = deriveGenerationResultContext(detail, operation);
          if (derived && resultSets.some((resultSet) => resultSet.artifacts.length > 0)) {
            context = { ...derived, resultSets };
          }
        }
        assetSummaries = assetList;
      } catch {
        // The run view still shows the completed run without candidates.
      }
    }
    setKeyframeRun({ shotId, detail, context });
    setAssets(assetSummaries);
  }

  function handleRunChange(next: WorkflowRunDetail) {
    if (!keyframeRun) return;
    if (next.run.status === "completed") {
      void applyRun(keyframeRun.shotId, next);
      return;
    }
    setKeyframeRun({ ...keyframeRun, detail: next });
  }

  /** Pin the already-saved version (the results dialog saved + approved it). */
  async function handlePinSavedVersion(shotId: string, versionId: string) {
    setPinningShotId(shotId);
    setActionError(null);
    try {
      await setShotKeyframe(projectRootPath, shotId, versionId);
      setKeyframeRun(null);
      await refresh();
      notifyChanged();
    } catch (caught: unknown) {
      setActionError(describeError(caught));
    } finally {
      setPinningShotId(null);
    }
  }

  return (
    <section
      aria-label="Scene shots"
      style={{ padding: "var(--space-16)", background: "var(--c-panel-soft)", border: "1px solid var(--c-hairline)", borderRadius: "var(--radius-lg)" }}
    >
      <header style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline", gap: "var(--space-12)" }}>
        <div>
          <h3 style={{ margin: 0, textTransform: "uppercase", fontSize: "var(--fs-md)", letterSpacing: "0.04em" }}>Shots</h3>
          <p style={{ margin: "var(--space-4) 0 0", fontSize: "var(--fs-md)", color: "var(--c-muted)" }}>
            The ordered moments of this scene. Generate a keyframe image for each shot before rendering video.
          </p>
        </div>
        <button type="button" onClick={() => setAdding((value) => !value)} aria-expanded={adding}>
          {adding ? "Cancel" : "Add Shot"}
        </button>
      </header>

      {error ? <p role="alert">{error}</p> : null}
      {actionError ? <p role="alert">{actionError}</p> : null}

      {adding ? (
        <div style={{ display: "grid", gap: "var(--space-8)", margin: "var(--space-12) 0", padding: "var(--space-12)", border: "1px solid var(--c-hairline)", borderRadius: "var(--radius-md)" }}>
          <label htmlFor="new-shot-intent">
            What happens in this shot (required)
            <input id="new-shot-intent" value={intent} onChange={(event) => setIntent(event.target.value)} placeholder="e.g. Establish the ops room" />
          </label>
          <div style={{ display: "flex", gap: "var(--space-8)", flexWrap: "wrap" }}>
            <label htmlFor="new-shot-duration">
              Duration (s)
              <input id="new-shot-duration" type="number" min="0.5" max="30" step="0.5" value={duration} onChange={(event) => setDuration(event.target.value)} />
            </label>
            <label htmlFor="new-shot-action">
              Action
              <input id="new-shot-action" value={action} onChange={(event) => setAction(event.target.value)} placeholder="e.g. Mara scans the console" />
            </label>
            <label htmlFor="new-shot-camera">
              Camera
              <input id="new-shot-camera" value={camera} onChange={(event) => setCamera(event.target.value)} placeholder="e.g. wide" />
            </label>
          </div>
          <div style={{ display: "flex", gap: "var(--space-8)" }}>
            <button type="button" onClick={() => void handleAdd()} disabled={pending || !intent.trim()}>
              {pending ? "Adding…" : "Create Shot"}
            </button>
            <button type="button" className="canon-secondary-button" onClick={() => setAdding(false)} disabled={pending}>
              Cancel
            </button>
          </div>
        </div>
      ) : null}

      {loading ? (
        <p role="status">Loading shots…</p>
      ) : shots.length === 0 ? (
        <div className="empty-state" role="status">
          <p>No shots yet</p>
          <p>Break this scene into moments — one row per shot. Add your first shot to make this scene renderable.</p>
        </div>
      ) : (
        <ol className="shot-list" style={{ margin: "var(--space-12) 0 0", paddingLeft: "var(--space-20)", display: "grid", gap: "var(--space-8)" }}>
          {shots.map((shot, index) => (
            <li key={shot.id} style={{ borderBottom: "1px solid var(--c-hairline)", paddingBottom: "var(--space-8)" }}>
              {editingId === shot.id ? (
                <div style={{ display: "grid", gap: "var(--space-8)" }}>
                  <label htmlFor={`shot-intent-${shot.id}`}>
                    What happens in this shot
                    <input id={`shot-intent-${shot.id}`} value={editIntent} onChange={(event) => setEditIntent(event.target.value)} />
                  </label>
                  <div style={{ display: "flex", gap: "var(--space-8)", flexWrap: "wrap" }}>
                    <label htmlFor={`shot-duration-${shot.id}`}>
                      Duration (s)
                      <input id={`shot-duration-${shot.id}`} type="number" min="0.5" max="30" step="0.5" value={editDuration} onChange={(event) => setEditDuration(event.target.value)} />
                    </label>
                    <label htmlFor={`shot-action-${shot.id}`}>
                      Action
                      <input id={`shot-action-${shot.id}`} value={editAction} onChange={(event) => setEditAction(event.target.value)} />
                    </label>
                    <label htmlFor={`shot-camera-${shot.id}`}>
                      Camera
                      <input id={`shot-camera-${shot.id}`} value={editCamera} onChange={(event) => setEditCamera(event.target.value)} />
                    </label>
                  </div>
                  <div style={{ display: "flex", gap: "var(--space-8)" }}>
                    <button type="button" onClick={() => void handleSaveEdit(shot.id)} disabled={pending}>
                      Save
                    </button>
                    <button type="button" className="canon-secondary-button" onClick={() => setEditingId(null)} disabled={pending}>
                      Cancel
                    </button>
                  </div>
                </div>
              ) : (
                <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}>
                  <div style={{ display: "flex", gap: "var(--space-8)", flexWrap: "wrap", alignItems: "baseline" }}>
                    <strong>
                      Shot {String(index + 1).padStart(2, "0")}
                    </strong>
                    <span>{shot.intent}</span>
                    <span style={{ color: "var(--c-muted)" }}>{shot.durationSeconds}s</span>
                    {shot.camera ? <span style={{ color: "var(--c-muted)" }}>· {shot.camera}</span> : null}
                    {shot.keyframeAssetVersionId ? (
                      <span className="asset-version-badge asset-version-badge--canonical">KEYFRAME PINNED</span>
                    ) : (
                      <span style={{ color: "var(--c-muted)", fontSize: "var(--fs-sm)" }}>no keyframe</span>
                    )}
                  </div>
                  {shot.action ? <span style={{ fontSize: "var(--fs-md)", color: "var(--c-muted)" }}>{shot.action}</span> : null}
                  <div className="shot-actions" style={{ display: "flex", gap: "var(--space-4)", flexWrap: "wrap" }}>
                    {!shot.keyframeAssetVersionId ? (
                      <button type="button" className="shot-actions__primary" onClick={() => void handleGenerateKeyframe(shot.id)} disabled={pending}>
                        Generate keyframe
                      </button>
                    ) : null}
                    <button type="button" className="asset-secondary-button" onClick={() => beginEdit(shot)} disabled={pending}>
                      Edit
                    </button>
                    <button type="button" className="asset-secondary-button shot-icon-action" onClick={() => void handleMove(shot.id, -1)} disabled={pending || index === 0} aria-label={`Move shot ${index + 1} up`} title="Move up">
                      ↑
                    </button>
                    <button type="button" className="asset-secondary-button shot-icon-action" onClick={() => void handleMove(shot.id, 1)} disabled={pending || index === shots.length - 1} aria-label={`Move shot ${index + 1} down`} title="Move down">
                      ↓
                    </button>
                    <button type="button" className="asset-secondary-button shot-icon-action shot-icon-action--danger" onClick={() => void handleDelete(shot.id)} disabled={pending} aria-label={`Delete shot ${index + 1}`} title="Delete shot">
                      ✕
                    </button>
                  </div>
                </div>
              )}
            </li>
          ))}
        </ol>
      )}

      {keyframeRun ? (
        <div style={{ marginTop: "var(--space-16)" }}>
          {keyframeRun.detail.run.status === "completed" && keyframeRun.context ? (
            <GenerationResults
              projectRootPath={projectRootPath}
              context={keyframeRun.context}
              assets={assets}
              saveActionLabel="Use this keyframe"
              defaultCanonical
              onPromoted={(_targetAssetId, versionId) => {
                void handlePinSavedVersion(keyframeRun.shotId, versionId);
              }}
            />
          ) : (
            <WorkflowRunView
              projectRootPath={projectRootPath}
              detail={keyframeRun.detail}
              onChange={handleRunChange}
            />
          )}
          {keyframeRun.detail.run.status === "completed" && keyframeRun.context ? (
            <p style={{ fontSize: "var(--fs-md)", color: "var(--c-muted)" }}>
              Pick the image you want as this shot&apos;s keyframe, then choose <strong>Use this keyframe</strong>. It becomes the approved version this scene renders with.
            </p>
          ) : null}
          {pinningShotId ? <p role="status">Pinning keyframe…</p> : null}
        </div>
      ) : null}
    </section>
  );
}
