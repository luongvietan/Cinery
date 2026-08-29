import { describe, expect, it } from "vitest";
import { StatefulTauriFacade } from "./StatefulTauriFacade";

describe("StatefulTauriFacade", () => {
  it("mutates subsequent list/detail responses after mutations", async () => {
    const facade = new StatefulTauriFacade();

    const asset = await facade.invoke<{ id: string }>("create_asset", { assetType: "outfit", label: "Mara Outfit", ownerEntityId: "mara" });
    const listed = await facade.invoke<Array<{ id: string }>>("list_assets", {});
    expect(listed.map((entry) => entry.id)).toContain(asset.id);

    const scene = await facade.invoke<{ id: string; worldAssetVersionId: string | null }>("create_scene", { title: "Scene 001", worldAssetVersionId: null });
    // Initial readiness has blockers; mutation clears them.
    const before = await facade.invoke<{ ready: boolean; blockers: Array<{ code: string }> }>("get_scene_readiness", { sceneId: scene.id });
    expect(before.ready).toBe(false);
    expect(before.blockers.map((blocker) => blocker.code)).toContain("missing_world");

    await facade.invoke("set_scene_world", { sceneId: scene.id, worldAssetVersionId: "world-v1" });
    const after = await facade.invoke<{ ready: boolean; blockers: Array<{ code: string }> }>("get_scene_readiness", { sceneId: scene.id });
    expect(after.blockers.map((blocker) => blocker.code)).not.toContain("missing_world");
  });

  it("rejects unknown commands with the normalized error shape", async () => {
    const facade = new StatefulTauriFacade();
    await expect(facade.invoke("make_everything_pass", {})).rejects.toMatchObject({ code: "UNKNOWN_COMMAND" });
  });

  it("keeps generated-artifact promotion idempotent and stateful", async () => {
    const facade = new StatefulTauriFacade();
    (facade as unknown as { state: { resultSets: Array<Record<string, unknown>> } }).state.resultSets.push({
      id: "result-1",
      workflowRunId: "run-1",
      artifacts: [{ id: "artifact-1", resultSetId: "result-1", ordinal: 1, captureStatus: "available" }],
      promotedToAssetId: {},
    });
    const asset = await facade.invoke<{ id: string }>("create_asset", { assetType: "face_lock", label: "Face" });

    const first = await facade.invoke<{ id: string }>("promote_generated_artifact", { artifactId: "artifact-1", targetAssetId: asset.id, setCanonical: true });
    const second = await facade.invoke<{ id: string }>("promote_generated_artifact", { artifactId: "artifact-1", targetAssetId: asset.id, setCanonical: true });
    expect(second.id).toBe(first.id);
  });

  it("persists compilation records across repeated queries", async () => {
    const facade = new StatefulTauriFacade();
    const scene = await facade.invoke<{ id: string }>("create_scene", { title: "S", worldAssetVersionId: "world-v1" });
    const compiled = await facade.invoke<{ id: string; exportSha256: string }>("compile_cinema", { sceneId: scene.id, totalDurationSeconds: 4 });
    expect(compiled.exportSha256).toHaveLength(64);
    expect(facade.state.compilations).toHaveLength(1);
  });
});
