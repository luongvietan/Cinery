/**
 * StatefulTauriFacade: an in-memory, stateful stand-in for the Tauri IPC
 * command surface. Unlike static canned responses, every mutation updates
 * the fixture state so subsequent list/detail calls reflect prior
 * create/promote/compile actions — mirroring the real backend contract.
 *
 * Command names mirror the CURRENT registered surface (lib.rs), i.e. the
 * unified Scene commands introduced in P9 (`create_world_scene`,
 * `assign_scene_world`, `add_world_scene_prop`, `set_shot_video`, ...).
 * Stale names must not accumulate here: the point of this facade is to
 * model the real command boundary, so unknown commands throw and new
 * backend commands must be added explicitly.
 */

type InvokeHandler = (args: Record<string, unknown>) => unknown | Promise<unknown>;

interface AssetVersionState {
  id: string;
  assetId: string;
  versionNumber: number;
  status: "candidate" | "canonical";
  createdAt: string;
}

interface AssetState {
  id: string;
  projectId: string;
  type: string;
  label: string;
  ownerEntityId: string | null;
  canonicalVersionId: string | null;
  versions: AssetVersionState[];
}

interface ResultSetState {
  id: string;
  workflowRunId: string;
  artifacts: Array<{
    id: string;
    resultSetId: string;
    ordinal: number;
    captureStatus: string;
    mediaKind?: "image" | "video";
    mimeType?: string;
  }>;
  promotedToAssetId: Record<string, string>;
}

interface WorkflowRunState {
  id: string;
  status: string;
  operationId: string;
  skillId: string;
  skillVersion: string;
  inputJson: string;
  createdAt: string;
}

interface QaCheckState {
  id: string;
  qaRunId: string;
  checkId: string;
  checkType: string;
  source: string;
  requirement: { label: string };
  status: string;
  confidence: number | null;
  observed: string;
  reason: string;
  repairHint: string | null;
  reviewStatus: string;
  reviewNote: string | null;
  reviewedAt: string | null;
  createdAt: string;
}

interface QaRunState {
  id: string;
  projectId: string;
  assetId: string;
  assetVersionId: string;
  mediaKind: "video";
  workflowRunId: string;
  status: string;
  overallStatus: string | null;
  adapterId: string;
  adapterVersion: string;
  modelId: string;
  executionLocation: string;
  checkPlan: Record<string, unknown>;
  contextSnapshot: Record<string, unknown>;
  rawResponseMetadata: null;
  errorCode: string | null;
  errorMessage: string | null;
  createdAt: string;
  startedAt: string | null;
  completedAt: string | null;
  checks: QaCheckState[];
}

/** P10.4 candidate read-model row served by the fixture's review commands. */
interface ShotVideoCandidateState {
  assetVersionId: string;
  versionNumber: number;
  shotId: string;
  sceneId: string;
  createdAt: string;
  filePath: string;
  mimeType: string;
  byteSize: number;
  reviewState: "active" | "rejected";
  isCanonical: boolean;
  qaOverallStatus: string | null;
  qaRunCount: number;
  providerId: string | null;
  modelId: string | null;
  workflowRunId: string | null;
  sourceAssetVersionId: string | null;
  sourceKeyframeIsCurrent: boolean;
}

export interface DesktopFixtureState {
  projectId: string;
  assets: AssetState[];  scenes: Array<{
    id: string;
    title: string;
    worldId: string | null;
    worldAssetVersionId: string | null;
    characters: Array<{ characterEntityId: string; lookAssetVersionId: string; sheetAssetVersionId: string | null }>;
    props: Array<{ propAssetVersionId: string }>;
    shots: Array<{
      id: string;
      ordering: number;
      durationSeconds: number;
      keyframeAssetVersionId: string | null;
      generatedVideoAssetVersionId: string | null;
      shotVideoCandidates?: ShotVideoCandidateState[];
    }>;
  }>;
  resultSets: ResultSetState[];
  compilations: Array<{ id: string; sceneId: string; exportSha256: string }>;
  workflowRuns: WorkflowRunState[];
  qaRuns: QaRunState[];
}

export class StatefulTauriFacade {
  private handlers = new Map<string, InvokeHandler>();
  readonly state: DesktopFixtureState;
  private counter = 0;
  /** P10.4 review state per candidate version id ("rejected" or absent). */
  private shotVideoReview = new Map<string, "rejected">();
  /** P10.4 promotion audit: version id -> promoted with qa_override. */
  private shotVideoPromotions = new Map<string, boolean>();

  constructor(projectId = "mara-project") {
    this.state = {
      projectId,
      assets: [],
      scenes: [],
      resultSets: [],
      compilations: [],
      workflowRuns: [],
      qaRuns: [],
    };
    this.registerHandlers();
  }

  private nextId(prefix: string): string {
    this.counter += 1;
    return `${prefix}-${this.counter}`;
  }

  snapshot(): Readonly<DesktopFixtureState> {
    return this.state;
  }

  private sceneSummary(scene: DesktopFixtureState["scenes"][number]) {
    return {
      id: scene.id,
      projectId: this.state.projectId,
      ordinal: 0,
      title: scene.title,
      summary: "",
      worldId: scene.worldId,
      worldAssetVersionId: scene.worldAssetVersionId,
      keyframeAssetId: null,
      createdAt: "now",
      updatedAt: "now",
    };
  }

  private workflowDetail(run: WorkflowRunState) {
    const approvalStatus = run.status === "waiting_for_approval"
      ? "waiting"
      : ["ready_for_execution", "running", "completed"].includes(run.status) ? "completed" : "pending";
    return {
      run: {
        ...run,
        projectId: this.state.projectId,
        prerequisiteReportJson: null,
        contextSnapshotJson: null,
        currentStepIndex: run.status === "completed" ? 6 : 4,
        failureCode: null,
        failureMessage: null,
        updatedAt: "now",
        completedAt: run.status === "completed" ? "now" : null,
      },
      steps: run.operationId === "asset.run_video_qa" ? [
        {
          id: `${run.id}-compile`, workflowRunId: run.id, stepDefinitionId: "compile-request",
          stepIndex: 2, stepType: "compile_request", status: "completed", inputJson: null,
          outputJson: JSON.stringify({
            executionLocation: "local", adapterId: "mock", modelId: "mock-video-qa",
            evidenceMode: "direct_video", request: { references: [] },
          }), startedAt: "now", completedAt: "now",
        },
        {
          id: `${run.id}-approval`, workflowRunId: run.id, stepDefinitionId: "approve-video-qa",
          stepIndex: 3, stepType: "approval", status: approvalStatus, inputJson: null,
          outputJson: null, startedAt: null, completedAt: approvalStatus === "completed" ? "now" : null,
        },
      ] : [],
      events: [],
      providerExecutions: [],
    };
  }

  private qaDetail(run: QaRunState) {
    const { checks, ...record } = run;
    return { run: record, checks };
  }

  private registerHandlers() {
    // --- assets ---
    this.handlers.set("create_asset", (args) => {
      const asset: AssetState = {
        id: this.nextId("asset"),
        projectId: this.state.projectId,
        type: String(args.assetType),
        label: String(args.label),
        ownerEntityId: (args.ownerEntityId as string) ?? null,
        canonicalVersionId: null,
        versions: [],
      };
      this.state.assets.push(asset);
      return asset;
    });

    this.handlers.set("list_assets", () =>
      this.state.assets.map((asset) => ({
        ...asset,
        versionCount: asset.versions.length,
        canonicalVersionNumber: asset.versions.find((v) => v.id === asset.canonicalVersionId)?.versionNumber ?? null,
        previewThumbnailPath: null,
        createdAt: "now",
        updatedAt: "now",
      })),
    );

    this.handlers.set("get_asset_with_versions", (args) => {
      const asset = this.state.assets.find((candidate) => candidate.id === args.assetId);
      if (!asset) throw { code: "ASSET_NOT_FOUND", message: "asset not found" };
      return { asset, versions: asset.versions };
    });

    this.handlers.set("promote_asset_version", (args) => {
      const versionId = String(args.assetVersionId);
      for (const asset of this.state.assets) {
        const version = asset.versions.find((candidate) => candidate.id === versionId);
        if (version) {
          asset.canonicalVersionId = version.id;
          version.status = "canonical";
          return { asset, promotedVersion: version, supersededVersionId: null };
        }
      }
      throw { code: "ASSET_VERSION_NOT_FOUND", message: "version not found" };
    });

    // --- P10.1 durable background provider jobs (observed, never executed here) ---
    this.handlers.set("list_provider_jobs", () => []);

    // --- shared workflow lifecycle + candidate-local Video QA ---
    this.handlers.set("create_workflow_run", (args) => {
      const input = (args.input as Record<string, unknown>) ?? {};
      const operationId = String(args.operationId);
      const inputJson = JSON.stringify(input);
      const existing = this.state.workflowRuns.find((run) =>
        run.operationId === operationId
        && run.inputJson === inputJson
        && !["completed", "cancelled", "failed", "rejected"].includes(run.status),
      );
      if (existing) return this.workflowDetail(existing);

      const run: WorkflowRunState = {
        id: this.nextId("workflow"),
        status: "created",
        operationId,
        skillId: String(args.skillId),
        skillVersion: String(args.skillVersion),
        inputJson,
        createdAt: `now-${this.counter}`,
      };
      this.state.workflowRuns.push(run);
      if (operationId === "asset.run_video_qa") {
        const assetVersionId = String(input.assetVersionId);
        const qaRunId = this.nextId("qa");
        this.state.qaRuns.push({
          id: qaRunId,
          projectId: this.state.projectId,
          assetId: "video-asset",
          assetVersionId,
          mediaKind: "video",
          workflowRunId: run.id,
          status: "queued",
          overallStatus: null,
          adapterId: String(input.adapterId ?? "mock"),
          adapterVersion: "1",
          modelId: String(input.modelId ?? "mock-video-qa"),
          executionLocation: "local",
          checkPlan: {
            schemaVersion: 1, assetId: "video-asset", assetVersionId, ownerEntityId: null,
            assetType: "video", referenceAssetVersionIds: [], checks: [{
              id: "video:integrity", checkType: "video_integrity", source: "artifact_detection",
              key: "integrity", label: "Video integrity", requirement: "Video decodes continuously.",
              validatorHint: null, blocking: true, referenceAssetVersionIds: [],
            }], createdAt: "now",
          },
          contextSnapshot: {},
          rawResponseMetadata: null,
          errorCode: null,
          errorMessage: null,
          createdAt: `now-${this.counter}`,
          startedAt: null,
          completedAt: null,
          checks: [],
        });
      }
      return this.workflowDetail(run);
    });

    this.handlers.set("list_workflow_runs", () => this.state.workflowRuns.map((run) => this.workflowDetail(run).run));

    this.handlers.set("get_workflow_run", (args) => {
      const run = this.state.workflowRuns.find((candidate) => candidate.id === args.workflowRunId);
      if (!run) throw { code: "WORKFLOW_RUN_NOT_FOUND", message: "workflow run not found" };
      return this.workflowDetail(run);
    });

    this.handlers.set("advance_workflow_run", (args) => {
      const run = this.state.workflowRuns.find((candidate) => candidate.id === args.workflowRunId);
      if (!run) throw { code: "WORKFLOW_RUN_NOT_FOUND", message: "workflow run not found" };
      if (run.status === "created") run.status = "waiting_for_approval";
      else if (run.status === "ready_for_execution") {
        run.status = "completed";
        const qaRun = this.state.qaRuns.find((candidate) => candidate.workflowRunId === run.id);
        if (qaRun) {
          qaRun.status = "succeeded";
          qaRun.overallStatus = "fail";
          qaRun.startedAt = "now";
          qaRun.completedAt = "now";
          qaRun.checks = [{
            id: this.nextId("qa-check"), qaRunId: qaRun.id, checkId: "video:integrity",
            checkType: "video_integrity", source: "artifact_detection", requirement: { label: "Video integrity" },
            status: "fail", confidence: 0.9, observed: "Decode discontinuity found.",
            reason: "A frame boundary is broken.", repairHint: null, reviewStatus: "unreviewed",
            reviewNote: null, reviewedAt: null, createdAt: "now",
          }];
        }
      }
      return this.workflowDetail(run);
    });

    this.handlers.set("approve_workflow_step", (args) => {
      const run = this.state.workflowRuns.find((candidate) => candidate.id === args.workflowRunId);
      if (!run) throw { code: "WORKFLOW_RUN_NOT_FOUND", message: "workflow run not found" };
      run.status = "ready_for_execution";
      return this.workflowDetail(run);
    });

    this.handlers.set("list_qa_runs", (args) => this.state.qaRuns
      .filter((run) => run.assetVersionId === args.assetVersionId)
      .map((run) => this.qaDetail(run).run));

    this.handlers.set("get_qa_run", (args) => {
      const run = this.state.qaRuns.find((candidate) => candidate.id === args.qaRunId);
      if (!run) throw { code: "QA_RUN_NOT_FOUND", message: "QA run not found" };
      return this.qaDetail(run);
    });

    this.handlers.set("review_qa_check", (args) => {
      const run = this.state.qaRuns.find((candidate) => candidate.id === args.qaRunId);
      const check = run?.checks.find((candidate) => candidate.checkId === args.checkId);
      if (!run || !check) throw { code: "QA_CHECK_NOT_FOUND", message: "QA check not found" };
      check.reviewStatus = String(args.reviewStatus);
      check.reviewNote = (args.note as string | null) ?? null;
      check.reviewedAt = "now";
      const effectiveStatuses = run.checks.map((candidate) => {
        if (candidate.reviewStatus === "overridden_pass") return "pass";
        if (candidate.reviewStatus === "overridden_fail") return "fail";
        return candidate.status;
      });
      run.overallStatus = effectiveStatuses.includes("fail") ? "fail"
        : effectiveStatuses.includes("uncertain") ? "needs_review" : "pass";
      return this.qaDetail(run);
    });

    // --- generation ---
    this.handlers.set("list_generation_results", (args) => {
      const runId = args.workflowRunId as string | undefined;
      return this.state.resultSets
        .filter((resultSet) => !runId || resultSet.workflowRunId === runId)
        .map((resultSet) => ({
          resultSet: {
            ...resultSet,
            projectId: this.state.projectId,
            providerAttemptId: "attempt",
            mediaKind: "image",
            requestedOutputCount: 4,
            workflowStepKey: "execute",
            createdAt: "now",
          },
          artifacts: resultSet.artifacts.map((artifact) => ({
            artifact: {
              ...artifact,
              mediaKind: artifact.mediaKind ?? "image",
              mimeType: artifact.mimeType ?? "image/png",
              width: 64,
              height: 64,
              byteSize: 10,
              sha256: "a".repeat(64),
              storagePath: "generations/x.png",
              captureErrorCode: null,
              createdAt: "now",
            },
            lineage: {
              artifactId: artifact.id,
              workflowRunId: resultSet.workflowRunId,
              workflowStepKey: "execute",
              workflowDefinitionId: "op",
              workflowVersion: "1.1.0",
              skillId: "character-builder",
              skillVersion: "1.1.0",
              compiledExecutionArtifactId: "c",
              compiledRequestSha256: "b".repeat(64),
              canonSnapshotId: null,
              canonSnapshotSha256: null,
              providerAttemptId: "attempt",
              providerId: "mock",
              modelId: "mock-image-v1",
              sourceAssetVersionIds: [],
              createdAt: "now",
            },
          })),
        }));
    });

    this.handlers.set("promote_generated_artifact", (args) => {
      const artifactId = String(args.artifactId);
      const targetAssetId = String(args.targetAssetId);
      const asset = this.state.assets.find((candidate) => candidate.id === targetAssetId);
      if (!asset) throw { code: "ASSET_NOT_FOUND", message: "target asset not found" };
      for (const resultSet of this.state.resultSets) {
        const artifact = resultSet.artifacts.find((candidate) => candidate.id === artifactId);
        if (artifact) {
          const promoted = resultSet.promotedToAssetId[artifactId];
          if (promoted) {
            const existingVersion = asset.versions.find((version) => version.id === promoted);
            return existingVersion ?? { id: promoted };
          }
          const version: AssetVersionState = {
            id: this.nextId("version"),
            assetId: asset.id,
            versionNumber: asset.versions.length + 1,
            status: args.setCanonical ? "canonical" : "candidate",
            createdAt: "now",
          };
          asset.versions.push(version);
          if (args.setCanonical) asset.canonicalVersionId = version.id;
          resultSet.promotedToAssetId[artifactId] = version.id;
          return version;
        }
      }
      throw { code: "GENERATION_ARTIFACT_NOT_PROMOTABLE", message: "artifact not found" };
    });

    // --- scenes (unified world_scenes command surface, P9/P10) ---
    this.handlers.set("list_world_scenes", () =>
      this.state.scenes.map((scene) => this.sceneSummary(scene)),
    );

    this.handlers.set("create_world_scene", (args) => {
      const scene = {
        id: this.nextId("scene"),
        title: String(args.title),
        worldId: null,
        worldAssetVersionId: null,
        characters: [],
        props: [],
        shots: [],
      };
      this.state.scenes.push(scene);
      return this.sceneSummary(scene);
    });

    this.handlers.set("get_world_scene", (args) => {
      const scene = this.state.scenes.find((candidate) => candidate.id === args.sceneId);
      if (!scene) throw { code: "SCENE_NOT_FOUND", message: "scene not found" };
      return this.sceneSummary(scene);
    });

    this.handlers.set("assign_scene_world", (args) => {
      const scene = this.state.scenes.find((candidate) => candidate.id === args.sceneId);
      if (!scene) throw { code: "SCENE_NOT_FOUND", message: "scene not found" };
      scene.worldId = (args.worldId as string) ?? null;
      scene.worldAssetVersionId = (args.worldAssetVersionId as string) ?? null;
      return null;
    });

    this.handlers.set("add_world_scene_character", (args) => {
      const scene = this.state.scenes.find((candidate) => candidate.id === args.sceneId);
      if (!scene) throw { code: "SCENE_NOT_FOUND", message: "scene not found" };
      const assignment = {
        characterEntityId: String(args.characterEntityId),
        lookAssetVersionId: String(args.lookAssetVersionId),
        sheetAssetVersionId: (args.sheetAssetVersionId as string) ?? null,
      };
      if (!scene.characters.some((c) => c.characterEntityId === assignment.characterEntityId)) {
        scene.characters.push(assignment);
      }
      return assignment;
    });

    this.handlers.set("add_world_scene_prop", (args) => {
      const scene = this.state.scenes.find((candidate) => candidate.id === args.sceneId);
      if (!scene) throw { code: "SCENE_NOT_FOUND", message: "scene not found" };
      const versionId = String(args.propAssetVersionId);
      if (!scene.props.some((prop) => prop.propAssetVersionId === versionId)) {
        scene.props.push({ propAssetVersionId: versionId });
      }
      return { propAssetVersionId: versionId };
    });

    this.handlers.set("get_scene_readiness", (args) => {
      const scene = this.state.scenes.find((candidate) => candidate.id === args.sceneId);
      if (!scene) throw { code: "SCENE_NOT_FOUND", message: "scene not found" };
      const blockers: Array<Record<string, unknown>> = [];
      if (!scene.worldAssetVersionId) {
        blockers.push({ code: "missing_world", sceneId: scene.id, entityId: null, shotId: null, message: "No World Plate is pinned to this scene.", actionTarget: "world" });
      }
      if (scene.characters.length === 0) {
        blockers.push({ code: "missing_cast_look", sceneId: scene.id, entityId: null, shotId: null, message: "No character is cast in this scene.", actionTarget: "cast" });
      }
      if (scene.shots.length === 0) {
        blockers.push({ code: "missing_shot", sceneId: scene.id, entityId: null, shotId: null, message: "This scene has no shots.", actionTarget: "shot" });
      }
      return { sceneId: scene.id, ready: blockers.length === 0, blockers };
    });

    this.handlers.set("compile_cinema", (args) => {
      const scene = this.state.scenes.find((candidate) => candidate.id === args.sceneId);
      if (!scene) throw { code: "SCENE_NOT_FOUND", message: "scene not found" };
      const compilation = {
        id: this.nextId("comp"),
        sceneId: scene.id,
        exportSha256: "c".repeat(64),
      };
      this.state.compilations.push(compilation);
      return { ...compilation, projectId: this.state.projectId, inputJson: "{}", compilationJson: "{}", exportPath: "prompts/cinema/compiled.md", createdAt: "now" };
    });

    // --- shots (cinema command surface) ---
    this.handlers.set("create_shot", (args) => {
      const scene = this.state.scenes.find((candidate) => candidate.id === args.sceneId);
      if (!scene) throw { code: "SCENE_NOT_FOUND", message: "scene not found" };
      const shot = {
        id: this.nextId("shot"),
        ordering: scene.shots.length,
        durationSeconds: Number(args.durationSeconds ?? 4),
        keyframeAssetVersionId: null as string | null,
        generatedVideoAssetVersionId: null as string | null,
      };
      scene.shots.push(shot);
      return { ...shot, sceneId: scene.id, intent: String(args.intent ?? "Establish"), action: null, camera: null, createdAt: "now", updatedAt: "now" };
    });

    this.handlers.set("list_shots", (args) => {
      const scene = this.state.scenes.find((candidate) => candidate.id === args.sceneId);
      if (!scene) throw { code: "SCENE_NOT_FOUND", message: "scene not found" };
      return scene.shots.map((shot) => ({
        ...shot,
        sceneId: scene.id,
        intent: "Establish",
        action: null,
        camera: null,
        createdAt: "now",
        updatedAt: "now",
      }));
    });

    this.handlers.set("set_shot_keyframe", (args) => {
      for (const scene of this.state.scenes) {
        const shot = scene.shots.find((candidate) => candidate.id === args.shotId);
        if (shot) {
          shot.keyframeAssetVersionId = (args.keyframeAssetVersionId as string) ?? null;
          return null;
        }
      }
      throw { code: "SHOT_NOT_FOUND", message: "shot not found" };
    });

    this.handlers.set("set_shot_video", (args) => {
      for (const scene of this.state.scenes) {
        const shot = scene.shots.find((candidate) => candidate.id === args.shotId);
        if (shot) {
          shot.generatedVideoAssetVersionId = (args.videoAssetVersionId as string) ?? null;
          return null;
        }
      }
      throw { code: "SHOT_NOT_FOUND", message: "shot not found" };
    });

    // Conflict-safe shot video promotion (P10.2 + P10.4 review gate): pins
    // the exact candidate version when the expected pin matches; replays are
    // no-ops; rejected candidates and QA failures demand explicit overrides.
    this.handlers.set("promote_shot_video_candidate", (args) => {
      for (const scene of this.state.scenes) {
        const shot = scene.shots.find((candidate) => candidate.id === args.shotId);
        if (shot) {
          const expected = (args.expectedCurrentVideoAssetVersionId as string | null) ?? null;
          if (shot.generatedVideoAssetVersionId !== expected) {
            throw { code: "PROMOTION_CONFLICT", message: "the Shot video changed before promotion completed" };
          }
          const assetVersionId = `video-version-${args.artifactId}`;
          const review = this.shotVideoReview.get(assetVersionId);
          if (review === "rejected") {
            throw { code: "CANDIDATE_REJECTED", message: "this video candidate was rejected and must be restored before promotion" };
          }
          const qaOverride = args.overrideReason != null && String(args.overrideReason).trim() !== "";
          this.shotVideoPromotions.set(assetVersionId, qaOverride);
          const previous = shot.generatedVideoAssetVersionId;
          shot.generatedVideoAssetVersionId = assetVersionId;
          return {
            shotId: shot.id,
            artifactId: args.artifactId,
            assetVersionId,
            previousAssetVersionId: previous,
          };
        }
      }
      throw { code: "SHOT_NOT_FOUND", message: "shot not found" };
    });

    this.handlers.set("list_shot_video_candidates", (args) => {
      for (const scene of this.state.scenes) {
        const shot = scene.shots.find((candidate) => candidate.id === args.shotId);
        if (shot) {
          return shot.shotVideoCandidates ?? [];
        }
      }
      throw { code: "SHOT_NOT_FOUND", message: "shot not found" };
    });

    this.handlers.set("resolve_canonical_shot_video", (args) => {
      for (const scene of this.state.scenes) {
        const shot = scene.shots.find((candidate) => candidate.id === args.shotId);
        if (shot) return shot.generatedVideoAssetVersionId;
      }
      throw { code: "SHOT_NOT_FOUND", message: "shot not found" };
    });

    this.handlers.set("reject_shot_video_candidate", (args) => {
      for (const scene of this.state.scenes) {
        const shot = scene.shots.find((candidate) => candidate.id === args.shotId);
        if (shot) {
          if (shot.generatedVideoAssetVersionId === args.assetVersionId) {
            throw {
              code: "CANONICAL_CANDIDATE_CANNOT_BE_REJECTED",
              message: "promote another video before rejecting the current canonical version",
            };
          }
          this.shotVideoReview.set(args.assetVersionId as string, "rejected");
          return "rejected";
        }
      }
      throw { code: "SHOT_NOT_FOUND", message: "shot not found" };
    });

    this.handlers.set("restore_shot_video_candidate", (args) => {
      for (const scene of this.state.scenes) {
        const shot = scene.shots.find((candidate) => candidate.id === args.shotId);
        if (shot) {
          this.shotVideoReview.delete(args.assetVersionId as string);
          return "active";
        }
      }
      throw { code: "SHOT_NOT_FOUND", message: "shot not found" };
    });
  }

  invoke<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
    const handler = this.handlers.get(command);
    if (!handler) {
      return Promise.reject({ code: "UNKNOWN_COMMAND", message: `no fixture handler for ${command}` });
    }
    return Promise.resolve().then(() => handler(args) as T);
  }
}
