import { useCallback, useEffect, useState } from "react";
import type { AssetSummary, OverviewAction, SceneDetail, SceneRecord } from "@cinematic/domain";
import { describeError } from "../../lib/errors";
import { ActionButton } from "../../components/ActionButton";
import { listAssets } from "../assets/api";
import { listCanonEntities } from "../canon/api";
import {
  compileCinema,
  createSceneFull,
  deleteShot,
  getScene,
  getSceneReadiness,
  listScenes,
  removeSceneProp,
  reorderShots,
  setSceneWorld,
  setShotKeyframe,
  updateShot,
  addSceneProp,
  type CinemaReadiness,
} from "./api";

interface CinemaSelection {
  sceneId: string | null;
  inspector:
    | { kind: "world" }
    | { kind: "cast"; characterId: string }
    | { kind: "prop"; versionId: string }
    | { kind: "shot"; shotId: string }
    | null;
}

export function CinemaWorkspace({ projectRootPath, action }: { projectRootPath: string; action: OverviewAction | null }) {
  const [assets, setAssets] = useState<AssetSummary[]>([]);
  const [scenes, setScenes] = useState<SceneRecord[]>([]);
  const [selection, setSelection] = useState<CinemaSelection>({ sceneId: null, inspector: null });
  const [detail, setDetail] = useState<SceneDetail | null>(null);
  const [readiness, setReadiness] = useState<CinemaReadiness | null>(null);
  const [compilation, setCompilation] = useState<{ id: string; exportPath: string; exportSha256: string } | null>(null);
  const [creating, setCreating] = useState(false);
  const [newTitle, setNewTitle] = useState("");
  const [pending, setPending] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const loadLists = useCallback(async () => {
    const [nextAssets, nextScenes] = await Promise.all([listAssets(projectRootPath), listScenes(projectRootPath)]);
    setAssets(nextAssets);
    setScenes(nextScenes);
  }, [projectRootPath]);

  useEffect(() => {
    setPending(true);
    loadLists()
      .then(async () => {
        if (action?.sceneId) {
          setSelection({ sceneId: action.sceneId, inspector: null });
        }
      })
      .catch((reason) => setError(describeError(reason)))
      .finally(() => setPending(false));
  }, [loadLists, action?.sceneId]);

  useEffect(() => {
    if (!selection.sceneId) {
      setDetail(null);
      setReadiness(null);
      return;
    }
    let cancelled = false;
    Promise.all([getScene(projectRootPath, selection.sceneId), getSceneReadiness(projectRootPath, selection.sceneId)])
      .then(([nextDetail, nextReadiness]) => {
        if (cancelled) return;
        setDetail(nextDetail);
        setReadiness(nextReadiness);
      })
      .catch((reason) => { if (!cancelled) setError(describeError(reason)); });
    return () => { cancelled = true; };
  }, [projectRootPath, selection.sceneId, busy]);

  async function selectScene(sceneId: string) {
    setSelection({ sceneId, inspector: null });
    setCompilation(null);
  }

  async function handleCreateScene() {
    setBusy(true); setError(null); setNotice(null);
    try {
      const scene = await createSceneFull(projectRootPath, newTitle.trim() || `Scene ${scenes.length + 1}`, null);
      setNotice(`${scene.title} created`);
      setNewTitle("");
      setCreating(false);
      await loadLists();
      await selectScene(scene.id);
    } catch (reason) { setError(describeError(reason)); } finally { setBusy(false); }
  }

  async function handleMutate(work: () => Promise<unknown>, success: string) {
    setBusy(true); setError(null); setNotice(null);
    try {
      await work();
      setNotice(success);
      if (selection.sceneId) {
        const [nextDetail, nextReadiness] = await Promise.all([
          getScene(projectRootPath, selection.sceneId),
          getSceneReadiness(projectRootPath, selection.sceneId),
        ]);
        setDetail(nextDetail);
        setReadiness(nextReadiness);
      }
    } catch (reason) { setError(describeError(reason)); } finally { setBusy(false); }
  }

  async function handleCompile(scene: SceneRecord) {
    await handleMutate(async () => {
      const sceneDetail = await getScene(projectRootPath, scene.id);
      const total = sceneDetail.shots.reduce((sum, shot) => sum + shot.durationSeconds, 0);
      const record = await compileCinema(projectRootPath, scene.id, total);
      setCompilation({ id: record.id, exportPath: record.exportPath, exportSha256: record.exportSha256 });
    }, "Cinema prompt compiled");
  }

  const worldAssets = assets.filter((asset) => asset.type === "world_plate");
  const propAssets = assets.filter((asset) => asset.type === "prop_plate");
  const keyframeAssets = assets.filter((asset) => asset.type === "shot_keyframe");
  const activeScene = scenes.find((scene) => scene.id === selection.sceneId) ?? null;
  const sceneBlocked = !readiness?.ready;

  return <section className="cinema-workspace" aria-label="Scene and cinema workspace">
    <header><span className="production-kicker">Production / Cinema</span><h2>Scenes & Cinema</h2><p>Assemble exact canonical references, resolve blockers, then compile a provider-neutral prompt.</p></header>
    {error ? <p role="alert">{error}</p> : null}{notice ? <p role="status">{notice}</p> : null}
    <div className="cinema-regions">
      <nav className="cinema-scene-list" aria-label="Scene list">
        {pending ? <p role="status">Loading scenes…</p> : null}
        {scenes.length === 0 && !pending ? <p role="status">No scenes yet. Create one to begin.</p> : null}
        <ul>
          {scenes.map((scene) => (
            <li key={scene.id}>
              <button type="button" aria-current={selection.sceneId === scene.id ? "true" : undefined} onClick={() => void selectScene(scene.id)}>
                <strong>{scene.title}</strong>
                <span>{scene.worldAssetVersionId ? "world pinned" : "no world"}</span>
              </button>
            </li>
          ))}
        </ul>
        {creating ? (
          <div className="cinema-create-scene">
            <label htmlFor="cinema-scene-title">Scene title</label>
            <input id="cinema-scene-title" value={newTitle} onChange={(event) => setNewTitle(event.target.value)} placeholder="Scene 001" disabled={busy} />
            <div className="production-form-actions">
              <button type="button" onClick={() => void handleCreateScene()} disabled={busy}>Create</button>
              <button type="button" className="production-secondary" onClick={() => setCreating(false)} disabled={busy}>Cancel</button>
            </div>
          </div>
        ) : (
          <button type="button" onClick={() => setCreating(true)}>Create Scene</button>
        )}
      </nav>

      {activeScene && detail ? (
        <div className="cinema-editor">
          <section aria-label="World">
            <h3>World</h3>
            <label htmlFor="cinema-world-select">World Plate</label>
            <select
              id="cinema-world-select"
              value={detail.scene.worldAssetVersionId ?? ""}
              onChange={(event) => void handleMutate(() => setSceneWorld(projectRootPath, detail.scene.id, event.target.value || null), "World reference updated")}
            >
              <option value="">No world pinned</option>
              {worldAssets.map((asset) => <option key={asset.id} value={asset.canonicalVersionId ?? ""}>{asset.label}</option>)}
            </select>
          </section>

          <section aria-label="Props">
            <h3>Props</h3>
            <ul>
              {detail.props.map((prop) => {
                const asset = assets.find((candidate) => candidate.canonicalVersionId === prop.propAssetVersionId);
                return (
                  <li key={prop.propAssetVersionId}>
                    <span>{asset?.label ?? prop.propAssetVersionId}</span>
                    <button type="button" onClick={() => void handleMutate(() => removeSceneProp(projectRootPath, detail.scene.id, prop.propAssetVersionId), "Prop removed")}>
                      Remove prop {asset?.label ?? prop.propAssetVersionId}
                    </button>
                  </li>
                );
              })}
            </ul>
            <label htmlFor="cinema-prop-select">Add prop</label>
            <select
              id="cinema-prop-select"
              value=""
              onChange={(event) => {
                const versionId = event.target.value;
                if (versionId) void handleMutate(() => addSceneProp(projectRootPath, detail.scene.id, versionId), "Prop added");
              }}
            >
              <option value="">Choose a canonical prop…</option>
              {propAssets
                .filter((asset) => asset.canonicalVersionId && !detail.props.some((prop) => prop.propAssetVersionId === asset.canonicalVersionId))
                .map((asset) => <option key={asset.id} value={asset.canonicalVersionId ?? ""}>{asset.label}</option>)}
            </select>
          </section>

          <section aria-label="Shots">
            <h3>Shots</h3>
            <ol>
              {detail.shots.map((shot, index) => (
                <li key={shot.id}>
                  <strong>{`Shot ${index + 1}`}</strong>
                  <label>
                    {`Duration for shot ${index + 1}`}
                    <input
                      type="number"
                      min={0.5}
                      max={30}
                      step={0.5}
                      value={shot.durationSeconds}
                      onChange={(event) => {
                        const duration = Number(event.target.value);
                        if (Number.isFinite(duration) && duration > 0) {
                          void handleMutate(() => updateShot(projectRootPath, shot.id, duration, null, shot.action, shot.camera), "Shot updated");
                        }
                      }}
                      disabled={busy}
                    />
                  </label>
                  <label>
                    {`Keyframe for shot ${index + 1}`}
                    <select
                      value={shot.keyframeAssetVersionId ?? ""}
                      onChange={(event) => void handleMutate(() => setShotKeyframe(projectRootPath, shot.id, event.target.value || null), "Keyframe updated")}
                    >
                      <option value="">No keyframe</option>
                      {keyframeAssets.map((asset) => <option key={asset.id} value={asset.canonicalVersionId ?? ""}>{asset.label}</option>)}
                    </select>
                  </label>
                  <button type="button" disabled={index === 0 || busy} onClick={() => {
                    const ordered = detail.shots.map((candidate) => candidate.id);
                    [ordered[index - 1], ordered[index]] = [ordered[index], ordered[index - 1]];
                    void handleMutate(() => reorderShots(projectRootPath, detail.scene.id, ordered), "Shots reordered");
                  }}>Move up</button>
                  <button type="button" disabled={index === detail.shots.length - 1 || busy} onClick={() => {
                    const ordered = detail.shots.map((candidate) => candidate.id);
                    [ordered[index + 1], ordered[index]] = [ordered[index], ordered[index + 1]];
                    void handleMutate(() => reorderShots(projectRootPath, detail.scene.id, ordered), "Shots reordered");
                  }}>Move down</button>
                  <button type="button" onClick={() => void handleMutate(() => deleteShot(projectRootPath, detail.scene.id, shot.id), "Shot deleted")}>Delete</button>
                </li>
              ))}
            </ol>
          </section>

          <section aria-label="Compile">
            <h3>Compile</h3>
            {readiness && !readiness.ready ? (
              <ul aria-label="Readiness blockers">
                {readiness.blockers.map((blocker) => (
                  <li key={`${blocker.code}-${blocker.entityId ?? ""}-${blocker.shotId ?? ""}`}>{blocker.message}</li>
                ))}
              </ul>
            ) : null}
            <ActionButton
              disabled={sceneBlocked || busy}
              disabledReason={sceneBlocked ? "Resolve all readiness blockers before compiling." : busy ? "Working…" : null}
              onClick={() => void handleCompile(detail.scene)}
            >
              Compile {detail.scene.title}
            </ActionButton>
            {compilation ? (
              <div className="cinema-compilation" role="status">
                <p>Compilation {compilation.id}</p>
                <p>Export: {compilation.exportPath}</p>
                <p>SHA-256: {compilation.exportSha256}</p>
              </div>
            ) : null}
          </section>
        </div>
      ) : (
        <div className="cinema-editor cinema-editor--empty">
          <p role="status">Select or create a scene to assemble its references.</p>
        </div>
      )}

      <aside className="cinema-inspector" aria-label="Reference inspector">
        <h3>Reference inspector</h3>
        {activeScene ? (
          <dl>
            <dt>Scene</dt><dd>{activeScene.title}</dd>
            <dt>World version</dt><dd>{detail?.scene.worldAssetVersionId ?? "not pinned"}</dd>
            <dt>Cast</dt><dd>{detail?.characters.length ?? 0}</dd>
            <dt>Props</dt><dd>{detail?.props.length ?? 0}</dd>
            <dt>Shots</dt><dd>{detail?.shots.length ?? 0}</dd>
            <dt>Ready</dt><dd>{readiness?.ready ? "yes" : "no"}</dd>
          </dl>
        ) : (
          <p>Select a scene to inspect its exact pinned versions.</p>
        )}
      </aside>
    </div>
  </section>;
}
