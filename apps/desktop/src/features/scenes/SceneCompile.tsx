import { useEffect, useState } from "react";
import { describeError } from "../../lib/errors";
import { listAssets } from "../assets/api";
import { GenerationResults } from "../generation/GenerationResults";
import {
  listGenerationResults,
} from "../generation/api";
import { ProviderModelFields } from "../providers/ProviderModelFields";
import { WorkflowRunView } from "../workflows/WorkflowRunView";
import {
  advanceWorkflowRun,
  createWorkflowRun,
} from "../workflows/api";
import { listSkillOperations } from "../workflows/api";
import { deriveGenerationResultContext, type GenerationResultContext } from "@cinematic/domain";
import type { AssetSummary, WorkflowRunDetail } from "@cinematic/domain";
import {
  compileCinema,
  getCompileReadiness,
  listCinemaCompilations,
  listShots,
  setShotVideo,
  type CinemaCompilation,
  type CompileReadiness,
  type Shot,
} from "./api";

interface SceneCompileProps {
  projectRootPath: string;
  sceneId: string;
  onChanged?: () => void;
}

/**
 * Compile/export section for the authoritative Scene: readiness blockers,
 * the compile action over the scene's shots, the persisted compilation
 * history with export artifacts, and scene video generation (P10.0) --
 * approval, execution, a reviewable candidate gallery, explicit promotion
 * into the scene's video asset, and an optional exact shot video pin.
 */
export function SceneCompile({ projectRootPath, sceneId, onChanged }: SceneCompileProps) {
  const [readiness, setReadiness] = useState<CompileReadiness | null>(null);
  const [compilations, setCompilations] = useState<CinemaCompilation[]>([]);
  const [shots, setShots] = useState<Shot[]>([]);
  const [totalDuration, setTotalDuration] = useState("8");
  const [compiling, setCompiling] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [lastCompilation, setLastCompilation] = useState<CinemaCompilation | null>(null);
  const [videoSelection, setVideoSelection] = useState({ providerId: "", modelId: "" });
  const [generatingVideo, setGeneratingVideo] = useState(false);
  const [videoRun, setVideoRun] = useState<WorkflowRunDetail | null>(null);
  const [videoContext, setVideoContext] = useState<GenerationResultContext | null>(null);
  const [assets, setAssets] = useState<AssetSummary[]>([]);
  const [pinning, setPinning] = useState(false);
  const [lastPromoted, setLastPromoted] = useState<{
    assetId: string;
    versionId: string;
  } | null>(null);

  useEffect(() => {
    let cancelled = false;
    setError(null);
    setLastCompilation(null);
    Promise.all([
      getCompileReadiness(projectRootPath, sceneId),
      listCinemaCompilations(projectRootPath, sceneId),
      listShots(projectRootPath, sceneId),
    ])
      .then(([nextReadiness, nextCompilations, nextShots]) => {
        if (cancelled) return;
        setReadiness(nextReadiness);
        setCompilations(nextCompilations);
        setShots(nextShots);
        const shotTotal = nextShots.reduce((sum, shot) => sum + shot.durationSeconds, 0);
        if (shotTotal > 0) {
          setTotalDuration(String(Math.min(120, Math.round(shotTotal * 100) / 100)));
        }
      })
      .catch((caught: unknown) => {
        if (!cancelled) setError(describeError(caught));
      });
    return () => {
      cancelled = true;
    };
  }, [projectRootPath, sceneId]);

  async function applyVideoRun(detail: WorkflowRunDetail) {
    setVideoRun(detail);
    setVideoContext(null);
    if (detail.run.status !== "completed") return;
    // Resolve the review state: candidates + the scene-owned video asset
    // from the persisted run and operation definition -- never local state.
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
          setVideoContext({ ...derived, resultSets });
        }
      }
      setAssets(assetList);
    } catch {
      // The run view still shows the completed run without candidates.
    }
  }

  async function handleGenerateVideo() {
    if (!videoSelection.providerId || !videoSelection.modelId) {
      setError("Connect an AI service before generating video.");
      return;
    }
    setGeneratingVideo(true);
    setError(null);
    try {
      // First run goes to the approval step; the run view below takes over.
      const created = await createWorkflowRun(
        projectRootPath,
        "scene-builder",
        "1.0.0",
        "scene.generate_video",
        {
          sceneId,
          providerId: videoSelection.providerId,
          modelId: videoSelection.modelId,
        },
      );
      const waiting = await advanceWorkflowRun(projectRootPath, created.run.id);
      await applyVideoRun(waiting);
      onChanged?.();
    } catch (caught: unknown) {
      setError(describeError(caught));
    } finally {
      setGeneratingVideo(false);
    }
  }

  function handleVideoRunChange(next: WorkflowRunDetail) {
    if (next.run.status === "completed") {
      void applyVideoRun(next);
      return;
    }
    setVideoRun(next);
  }

  /** Pin the just-promoted canonical version as the shot's exact video. */
  async function handlePinVideo(shotId: string, versionId: string) {
    setPinning(true);
    setError(null);
    try {
      await setShotVideo(projectRootPath, shotId, versionId);
      const nextShots = await listShots(projectRootPath, sceneId);
      setShots(nextShots);
      setLastPromoted(null);
      onChanged?.();
    } catch (caught: unknown) {
      setError(describeError(caught));
    } finally {
      setPinning(false);
    }
  }

  async function handleCompile() {
    const seconds = Number(totalDuration);
    if (!Number.isFinite(seconds) || seconds < 1 || seconds > 120) {
      setError("Total duration must be between 1 and 120 seconds");
      return;
    }
    setCompiling(true);
    setError(null);
    try {
      const compilation = await compileCinema(projectRootPath, sceneId, seconds);
      setLastCompilation(compilation);
      setCompilations(
        await listCinemaCompilations(projectRootPath, sceneId),
      );
      setReadiness(await getCompileReadiness(projectRootPath, sceneId));
      onChanged?.();
    } catch (caught: unknown) {
      setError(describeError(caught));
    } finally {
      setCompiling(false);
    }
  }

  const pinTargetShots = shots.filter(
    (shot) => shot.generatedVideoAssetVersionId === null,
  );

  return (
    <section
      aria-label="Scene compile"
      style={{ padding: "var(--space-16)", background: "var(--c-panel-soft)", border: "1px solid var(--c-hairline)", borderRadius: "var(--radius-lg)" }}
    >
      <header>
        <h3 style={{ margin: 0, textTransform: "uppercase", fontSize: "var(--fs-md)", letterSpacing: "0.04em" }}>Render</h3>
        <p style={{ margin: "var(--space-4) 0 0", fontSize: "var(--fs-md)", color: "var(--c-muted)" }}>
          Compile this scene into a ready-to-send video request, then generate the video.
        </p>
      </header>

      {error ? <p role="alert">{error}</p> : null}

      {readiness && !readiness.ready ? (
        <div role="status" style={{ margin: "var(--space-12) 0" }}>
          <p style={{ margin: "0 0 var(--space-4)", fontWeight: 600 }}>Before this scene can render:</p>
          <ul style={{ margin: 0, paddingLeft: "var(--space-20)" }}>
            {readiness.blockers.map((blocker) => (
              <li key={`${blocker.code}-${blocker.shotId ?? blocker.entityId ?? "scene"}`}>{blocker.message}</li>
            ))}
          </ul>
        </div>
      ) : null}

      <div style={{ display: "flex", gap: "var(--space-8)", alignItems: "end", flexWrap: "wrap", marginTop: "var(--space-12)" }}>
        <label htmlFor="compile-duration">
          Total runtime (s)
          <input
            id="compile-duration"
            type="number"
            min="1"
            max="120"
            step="0.5"
            value={totalDuration}
            onChange={(event) => setTotalDuration(event.target.value)}
          />
        </label>
        <button
          type="button"
          onClick={() => void handleCompile()}
          disabled={compiling || (readiness ? !readiness.ready : false)}
          title={readiness && !readiness.ready ? "Resolve the readiness blockers first" : undefined}
        >
          {compiling ? "Compiling…" : "Compile Scene"}
        </button>
      </div>

      {lastCompilation ? (
        <div style={{ marginTop: "var(--space-12)", fontSize: "var(--fs-md)" }}>
          <p style={{ margin: "0 0 var(--space-4)", fontWeight: 600 }}>Latest compilation</p>
          <p style={{ margin: 0 }}>Export: {lastCompilation.exportPath}</p>
          <p style={{ margin: 0 }}>SHA-256: {lastCompilation.exportSha256}</p>
        </div>
      ) : null}

      {compilations.length > 0 ? (
        <div className="scene-video-generate" style={{ marginTop: "var(--space-16)", paddingTop: "var(--space-12)", borderTop: "1px solid var(--c-hairline)" }}>
          <h4 style={{ margin: "0 0 var(--space-4)" }}>Generate video</h4>
          <p style={{ margin: "0 0 var(--space-8)", fontSize: "var(--fs-md)", color: "var(--c-muted)" }}>
            Animate the latest compiled prompt into a real video through a connected AI service. The request is
            shown for approval before anything is generated.
          </p>
          <ProviderModelFields
            projectRootPath={projectRootPath}
            value={videoSelection}
            mediaType="video"
            requiresReferences={false}
            onChange={setVideoSelection}
          />
          <button
            type="button"
            style={{ marginTop: "var(--space-8)" }}
            onClick={() => void handleGenerateVideo()}
            disabled={generatingVideo || compilations.length === 0}
          >
            {generatingVideo ? "Preparing video request…" : "Generate video from latest compilation"}
          </button>
        </div>
      ) : null}

      {videoRun ? (
        <div style={{ marginTop: "var(--space-16)" }}>
          <WorkflowRunView
            projectRootPath={projectRootPath}
            detail={videoRun}
            onChange={handleVideoRunChange}
          />
        </div>
      ) : null}

      {videoContext ? (
        <div style={{ marginTop: "var(--space-12)" }}>
          <GenerationResults
            projectRootPath={projectRootPath}
            context={videoContext}
            assets={assets}
            saveActionLabel="Save Video to Assets"
            onPromoted={(targetAssetId, versionId) => {
              setLastPromoted({ assetId: targetAssetId, versionId });
            }}
          />
          {lastPromoted && pinTargetShots.length > 0 ? (
            <div style={{ marginTop: "var(--space-8)", fontSize: "var(--fs-md)", color: "var(--c-muted)" }}>
              {pinning ? "Pinning video to shot…" : "Saved. Pin this video as the exact version for:"}
              <div style={{ display: "flex", gap: "var(--space-8)", flexWrap: "wrap", marginTop: "var(--space-4)" }}>
                {pinTargetShots.map((shot) => (
                  <button
                    key={shot.id}
                    type="button"
                    className="production-secondary"
                    disabled={pinning}
                    onClick={() => void handlePinVideo(shot.id, lastPromoted.versionId)}
                  >
                    Shot {shot.ordering + 1}
                  </button>
                ))}
              </div>
            </div>
          ) : lastPromoted && pinTargetShots.length === 0 ? (
            <p style={{ marginTop: "var(--space-8)", fontSize: "var(--fs-md)", color: "var(--c-muted)" }}>
              Every shot already has an exact video version pinned. Promoting a newer video never changes those pins
              — restage a shot explicitly to update it.
            </p>
          ) : null}
        </div>
      ) : null}

      {shots.some((shot) => shot.generatedVideoAssetVersionId !== null) ? (
        <div style={{ marginTop: "var(--space-12)", fontSize: "var(--fs-md)" }}>
          <p style={{ margin: "0 0 var(--space-4)", fontWeight: 600 }}>Pinned shot videos</p>
          <ul style={{ margin: 0, paddingLeft: "var(--space-20)" }}>
            {shots
              .filter((shot) => shot.generatedVideoAssetVersionId !== null)
              .map((shot) => (
                <li key={shot.id}>
                  Shot {shot.ordering + 1} — exact version {shot.generatedVideoAssetVersionId}
                </li>
              ))}
          </ul>
        </div>
      ) : null}

      {compilations.length > 0 ? (
        <div style={{ marginTop: "var(--space-12)" }}>
          <h4 style={{ margin: "0 0 var(--space-4)" }}>Compilation history</h4>
          <ul style={{ margin: 0, paddingLeft: "var(--space-20)", fontSize: "var(--fs-md)" }}>
            {compilations.map((compilation) => (
              <li key={compilation.id}>
                {new Date(compilation.createdAt).toLocaleString()} — {compilation.exportPath} (sha {compilation.exportSha256.slice(0, 12)}…)
              </li>
            ))}
          </ul>
        </div>
      ) : null}
    </section>
  );
}
