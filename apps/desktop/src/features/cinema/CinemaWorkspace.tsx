import { useEffect, useState } from "react";
import type { AssetSummary, OverviewAction, SceneRecord } from "@cinematic/domain";
import { describeError } from "../../lib/errors";
import { ActionButton } from "../../components/ActionButton";
import { listAssets } from "../assets/api";
import { listCanonEntities } from "../canon/api";
import { compileCinema, getScene, listScenes, stageScene as stageSceneCommand } from "./api";

export function CinemaWorkspace({ projectRootPath, action }: { projectRootPath: string; action: OverviewAction | null }) {
  const [assets, setAssets] = useState<AssetSummary[]>([]);
  const [scenes, setScenes] = useState<SceneRecord[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function load() {
    try {
      const [nextAssets, nextScenes] = await Promise.all([listAssets(projectRootPath), listScenes(projectRootPath)]);
      setAssets(nextAssets); setScenes(nextScenes);
    } catch (reason) { setError(describeError(reason)); } finally { setLoaded(true); }
  }
  useEffect(() => { void load(); }, [projectRootPath]);

  async function stageScene() {
    setBusy(true); setError(null); setNotice(null);
    try {
      const characters = await listCanonEntities(projectRootPath, "character");
      const character = characters.find((item) => item.id === action?.characterEntityId) ?? characters[0];
      const world = assets.find((item) => item.type === "world_plate" && item.canonicalVersionId);
      const look = assets.find((item) => item.type === "outfit" && item.ownerEntityId === character?.id && item.canonicalVersionId);
      const sheet = assets.find((item) => item.type === "character_sheet" && item.ownerEntityId === character?.id && item.canonicalVersionId);
      if (!character || !world?.canonicalVersionId || !look?.canonicalVersionId || !sheet?.canonicalVersionId) throw new Error("A canonical world, look, and sheet are required to stage a scene.");
      const scene = await stageSceneCommand(projectRootPath, `Scene ${String(scenes.length + 1).padStart(3, "0")}`, world.canonicalVersionId, character.id, look.canonicalVersionId, sheet.canonicalVersionId);
      setNotice(`${scene.title} staged`); await load();
    } catch (reason) { setError(describeError(reason)); } finally { setBusy(false); }
  }

  async function compile(scene: SceneRecord) {
    setBusy(true); setError(null); setNotice(null);
    try {
      const detail = await getScene(projectRootPath, scene.id);
      const total = detail.shots.reduce((sum, shot) => sum + shot.durationSeconds, 0);
      await compileCinema(projectRootPath, scene.id, total);
      setNotice("Cinema prompt compiled");
    } catch (reason) { setError(describeError(reason)); } finally { setBusy(false); }
  }

  const scopedScene = action?.sceneId ? scenes.filter((scene) => scene.id === action.sceneId) : scenes;
  const stageBlockedReason = busy
    ? "Finishing the current scene action…"
    : null;
  return <section className="cinema-workspace" aria-label="Scene and cinema workspace">
    <header><span className="production-kicker">Production / Cinema</span><h2>Scenes & Cinema</h2><p>Stage exact canonical references, then compile durable provider-neutral prompts.</p></header>
    {error ? <p role="alert">{error}</p> : null}{notice ? <p role="status">{notice}</p> : null}
    {action?.id === "scene" || action?.id === "restage_scene" ? (
      <span className="action-button-blocked">
        <ActionButton disabled={busy} disabledReason={stageBlockedReason} onClick={() => void stageScene()}>{action.id === "restage_scene" ? "Restage Scene" : "Stage Scene"}</ActionButton>
      </span>
    ) : null}
    {loaded && !error && scopedScene.length === 0 ? <p role="status">No scenes yet. Stage a scene from the production progress panel.</p> : null}
    <ul>{scopedScene.map((scene) => <li key={scene.id}><strong>{scene.title}</strong><ActionButton disabled={busy} disabledReason={busy ? "Finishing the current scene action…" : null} onClick={() => void compile(scene)}>Compile {scene.title}</ActionButton></li>)}</ul>
  </section>;
}
