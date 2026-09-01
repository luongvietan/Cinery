import { describe, expect, it } from "vitest";
import { StatefulTauriFacade } from "./StatefulTauriFacade";

describe("StatefulTauriFacade", () => {
  it("mutates subsequent list/detail responses after mutations", async () => {
    const facade = new StatefulTauriFacade();

    const asset = await facade.invoke<{ id: string }>("create_asset", { assetType: "outfit", label: "Mara Outfit", ownerEntityId: "mara" });
    const listed = await facade.invoke<Array<{ id: string }>>("list_assets", {});
    expect(listed.map((entry) => entry.id)).toContain(asset.id);

    const scene = await facade.invoke<{ id: string; worldAssetVersionId: string | null }>("create_world_scene", { title: "Scene 001", summary: "" });
    // Initial readiness has blockers; mutation clears them.
    const before = await facade.invoke<{ ready: boolean; blockers: Array<{ code: string }> }>("get_scene_readiness", { sceneId: scene.id });
    expect(before.ready).toBe(false);
    expect(before.blockers.map((blocker) => blocker.code)).toContain("missing_world");

    await facade.invoke("assign_scene_world", { sceneId: scene.id, worldId: "world-1", worldAssetVersionId: "world-v1" });
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
    const scene = await facade.invoke<{ id: string }>("create_world_scene", { title: "S", summary: "" });
    const compiled = await facade.invoke<{ id: string; exportSha256: string }>("compile_cinema", { sceneId: scene.id, totalDurationSeconds: 4 });
    expect(compiled.exportSha256).toHaveLength(64);
    expect(facade.state.compilations).toHaveLength(1);
  });

  it("models the shot video pin on the current command surface", async () => {
    const facade = new StatefulTauriFacade();
    const scene = await facade.invoke<{ id: string }>("create_world_scene", { title: "S", summary: "" });
    const shot = await facade.invoke<{ id: string }>("create_shot", { sceneId: scene.id, durationSeconds: 4, intent: "Establish" });

    await facade.invoke("set_shot_video", { shotId: shot.id, videoAssetVersionId: "video-v1" });
    const shots = await facade.invoke<Array<{ id: string; generatedVideoAssetVersionId: string | null }>>("list_shots", { sceneId: scene.id });
    expect(shots.find((candidate) => candidate.id === shot.id)?.generatedVideoAssetVersionId).toBe("video-v1");
  });

  it("promotes the shot video candidate conflict-safely against the expected pin", async () => {
    const facade = new StatefulTauriFacade();
    const scene = await facade.invoke<{ id: string }>("create_world_scene", { title: "S", summary: "" });
    const shot = await facade.invoke<{ id: string }>("create_shot", { sceneId: scene.id, durationSeconds: 4, intent: "Establish" });

    const promoted = await facade.invoke<{ shotId: string; artifactId: string; assetVersionId: string; previousAssetVersionId: string | null }>(
      "promote_shot_video_candidate",
      { shotId: shot.id, artifactId: "artifact-1", expectedCurrentVideoAssetVersionId: null },
    );
    expect(promoted).toMatchObject({ shotId: shot.id, artifactId: "artifact-1", previousAssetVersionId: null });
    expect(promoted.assetVersionId).toBe("video-version-artifact-1");

    // Replay with the matching expected pin is idempotent.
    const replayed = await facade.invoke<{ assetVersionId: string }>(
      "promote_shot_video_candidate",
      { shotId: shot.id, artifactId: "artifact-1", expectedCurrentVideoAssetVersionId: promoted.assetVersionId },
    );
    expect(replayed.assetVersionId).toBe(promoted.assetVersionId);

    // A stale expected pin conflicts without overwriting the winner.
    await expect(
      facade.invoke("promote_shot_video_candidate", { shotId: shot.id, artifactId: "artifact-2", expectedCurrentVideoAssetVersionId: null }),
    ).rejects.toMatchObject({ code: "PROMOTION_CONFLICT" });
  });

  it("models generic candidate-local Video QA workflow, rerun, and raw-preserving review", async () => {
    const facade = new StatefulTauriFacade();
    const input = { assetVersionId: "video-v1", adapterId: "mock" };

    const first = await facade.invoke<{ run: { id: string; status: string } }>("create_workflow_run", {
      skillId: "video-qa", skillVersion: "1.0.0", operationId: "asset.run_video_qa", input,
    });
    const duplicate = await facade.invoke<{ run: { id: string } }>("create_workflow_run", {
      skillId: "video-qa", skillVersion: "1.0.0", operationId: "asset.run_video_qa", input,
    });
    expect(duplicate.run.id).toBe(first.run.id);
    expect(await facade.invoke("list_qa_runs", { assetVersionId: "video-v2" })).toEqual([]);

    const waiting = await facade.invoke<{ run: { status: string } }>("advance_workflow_run", { workflowRunId: first.run.id });
    expect(waiting.run.status).toBe("waiting_for_approval");
    await facade.invoke("approve_workflow_step", { workflowRunId: first.run.id, stepDefinitionId: "approve-video-qa" });
    const completed = await facade.invoke<{ run: { status: string } }>("advance_workflow_run", { workflowRunId: first.run.id });
    expect(completed.run.status).toBe("completed");

    const history = await facade.invoke<Array<{ id: string; assetVersionId: string; overallStatus: string }>>("list_qa_runs", { assetVersionId: "video-v1" });
    expect(history).toHaveLength(1);
    expect(history[0]).toMatchObject({ assetVersionId: "video-v1", overallStatus: "fail" });
    const detail = await facade.invoke<{ checks: Array<{ checkId: string; status: string }> }>("get_qa_run", { qaRunId: history[0].id });
    expect(detail.checks[0]).toMatchObject({ checkId: "video:integrity", status: "fail" });

    const reviewed = await facade.invoke<{ run: { overallStatus: string }; checks: Array<{ status: string; reviewStatus: string }> }>("review_qa_check", {
      qaRunId: history[0].id, checkId: "video:integrity", reviewStatus: "overridden_pass", note: "Reviewed frames",
    });
    expect(reviewed.run.overallStatus).toBe("pass");
    expect(reviewed.checks[0]).toMatchObject({ status: "fail", reviewStatus: "overridden_pass" });

    const rerun = await facade.invoke<{ run: { id: string } }>("create_workflow_run", {
      skillId: "video-qa", skillVersion: "1.0.0", operationId: "asset.run_video_qa", input,
    });
    expect(rerun.run.id).not.toBe(first.run.id);
    await expect(facade.invoke("create_video_qa_workflow", { assetVersionId: "video-v1" }))
      .rejects.toMatchObject({ code: "UNKNOWN_COMMAND" });
  });
});
