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

export interface DesktopFixtureState {
  projectId: string;
  assets: AssetState[];
  scenes: Array<{
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
    }>;
  }>;
  resultSets: ResultSetState[];
  compilations: Array<{ id: string; sceneId: string; exportSha256: string }>;
  workflowRuns: Array<{ id: string; status: string; operationId: string }>;
}

export class StatefulTauriFacade {
  private handlers = new Map<string, InvokeHandler>();
  readonly state: DesktopFixtureState;
  private counter = 0;

  constructor(projectId = "mara-project") {
    this.state = {
      projectId,
      assets: [],
      scenes: [],
      resultSets: [],
      compilations: [],
      workflowRuns: [],
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
  }

  invoke<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
    const handler = this.handlers.get(command);
    if (!handler) {
      return Promise.reject({ code: "UNKNOWN_COMMAND", message: `no fixture handler for ${command}` });
    }
    return Promise.resolve().then(() => handler(args) as T);
  }
}
