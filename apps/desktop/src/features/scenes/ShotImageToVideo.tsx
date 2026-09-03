import { useEffect, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import type { WorkflowRunDetail } from "@cinematic/domain";
import { describeError } from "../../lib/errors";
import {
  getShotImageToVideoSource,
  promoteShotVideoCandidate,
  type Shot,
} from "./api";
import { advanceWorkflowRun, createWorkflowRun, getWorkflowRun, listSkillOperations, listWorkflowRuns } from "../workflows/api";
import { listGenerationResults } from "../generation/api";
import { ProviderModelFields, type ProviderModelSelection } from "../providers/ProviderModelFields";
import { joinProjectRelativePath } from "../assets/paths";
import { GenerationResultCard } from "../generation/GenerationResultCard";
import { WorkflowRunView } from "../workflows/WorkflowRunView";
import type { GeneratedArtifactDetail, GenerationResultSetDetail } from "@cinematic/domain";
import { getAssetWithVersions, listAssets } from "../assets/api";
import { VideoQaPanel } from "../qa/VideoQaPanel";

interface ShotImageToVideoProps {
  projectRootPath: string;
  sceneId: string;
  shot: Shot;
  onShotChanged(): void;
}

interface ResultState {
  detail: WorkflowRunDetail;
  artifacts: Array<{
    detail: GeneratedArtifactDetail;
    assetVersionId: string | null;
    versionNumber: number | null;
  }>;
}

function isActiveRun(run: ResultState | null): boolean {
  return run !== null && !["completed", "cancelled", "failed", "rejected"].includes(run.detail.run.status);
}

function parseDurationSeconds(value: string): number | null {
  const durationSeconds = Number(value);
  return Number.isFinite(durationSeconds) && durationSeconds >= 0.5 && durationSeconds <= 30
    ? durationSeconds
    : null;
}

/**
 * Shot-local image-to-video panel: pick an I2V-capable AI service, animate
 * the shot's exact pinned keyframe, review the video candidates, then pin
 * one exact version with "Use for Shot" (conflict-safe against the current
 * pin).
 */
export function ShotImageToVideo({ projectRootPath, sceneId, shot, onShotChanged }: ShotImageToVideoProps) {
  const [source, setSource] = useState<{
    assetVersionId: string;
    thumbnailPath: string | null;
    versionNumber: number;
  } | null>(null);
  const [sourceMissing, setSourceMissing] = useState(false);
  const [selection, setSelection] = useState<ProviderModelSelection>({ providerId: "", modelId: "" });
  const [prompt, setPrompt] = useState("");
  const [duration, setDuration] = useState(String(shot.durationSeconds));
  const [creating, setCreating] = useState(false);
  const creatingRef = useRef(false);
  const [runs, setRuns] = useState<ResultState[]>([]);
  const [promoting, setPromoting] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  // The frozen keyframe the run will use (display-only projection).
  useEffect(() => {
    let cancelled = false;
    setSource(null);
    setSourceMissing(shot.keyframeAssetVersionId === null);
    if (shot.keyframeAssetVersionId === null) {
      return;
    }
    getShotImageToVideoSource(projectRootPath, shot.id)
      .then((resolved) => {
        if (!cancelled) setSource(resolved);
      })
      .catch(() => {
        if (!cancelled) setSourceMissing(true);
      });
    return () => {
      cancelled = true;
    };
  }, [projectRootPath, shot.id, shot.keyframeAssetVersionId]);

  // Durable status restoration retains every persisted I2V run for this Shot
  // so candidate-local QA history remains available after later generations.
  useEffect(() => {
    let cancelled = false;
    listWorkflowRuns(projectRootPath)
      .then((records) => {
        if (cancelled) return null;
        const matching = records
          .filter((record) => record.operationId === "shot.image_to_video")
          .filter((record) => {
            try {
              const parsed = JSON.parse(record.inputJson) as Record<string, unknown>;
              return parsed.shotId === shot.id;
            } catch {
              return false;
            }
          })
          .sort((a, b) => b.createdAt.localeCompare(a.createdAt));
        return Promise.all(matching.map((record) => getWorkflowRun(projectRootPath, record.id)));
      })
      .then((details) => details ? Promise.all(details.map(resolveRun)) : [])
      .then((restored) => {
        if (!cancelled) setRuns(restored);
      })
      .catch(() => {
        // Keep any previously-restored run; never surface a transient read
        // failure as a generation error.
      });
    return () => {
      cancelled = true;
    };
    // Remount semantics: restore once per shot identity.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectRootPath, shot.id]);

  function resetCreateGuard() {
    creatingRef.current = false;
    setCreating(false);
  }

  async function handleGenerate() {
    // Synchronous double-click guard: while a creation is in flight (ref)
    // or a run is active (state), further clicks never create a second
    // payload. Terminal runs stay visible but do not block a new generation.
    const trimmedPrompt = prompt.trim();
    const durationSeconds = parseDurationSeconds(duration);
    if (
      creatingRef.current
      || runs.some(isActiveRun)
      || !source
      || !selection.providerId
      || !selection.modelId
      || !trimmedPrompt
      || durationSeconds === null
    ) return;
    creatingRef.current = true;
    setCreating(true);
    setError(null);
    try {
      const created = await createWorkflowRun(projectRootPath, "scene-builder", "1.0.0", "shot.image_to_video", {
        sceneId,
        shotId: shot.id,
        providerId: selection.providerId,
        modelId: selection.modelId,
        prompt: trimmedPrompt,
        generationParameters: { durationSeconds },
      });
      const waiting = await advanceWorkflowRun(projectRootPath, created.run.id);
      const resolved = await resolveRun(waiting);
      setRuns((current) => [resolved, ...current.filter((run) => run.detail.run.id !== resolved.detail.run.id)]);
    } catch (caught: unknown) {
      setError(describeError(caught));
    } finally {
      resetCreateGuard();
    }
  }

  /** Resolve candidates once a run completes so the user reviews actual
   * videos before pinning. */
  async function resolveRun(detail: WorkflowRunDetail): Promise<ResultState> {
    if (detail.run.status !== "completed") {
      return { detail, artifacts: [] };
    }
    let artifacts: GeneratedArtifactDetail[] = [];
    try {
      const resultSets = await listGenerationResults(projectRootPath, detail.run.id);
      artifacts = resultSets.flatMap((resultSet: GenerationResultSetDetail) => resultSet.artifacts);
    } catch {
      // The run view still shows the completed run without candidates.
    }
    const candidates = artifacts.map((artifact) => ({
      detail: artifact,
      assetVersionId: null as string | null,
      versionNumber: null as number | null,
    }));
    if (candidates.length === 0) return { detail, artifacts: candidates };
    try {
      const assets = await listAssets(projectRootPath);
      const videoAssets = assets.filter((asset) => asset.type === "video" && asset.ownerEntityId === sceneId);
      const versionSets = await Promise.all(
        videoAssets.map((asset) => getAssetWithVersions(projectRootPath, asset.id)),
      );
      for (const candidate of candidates) {
        const version = versionSets
          .flatMap((asset) => asset.versions)
          .find((entry) => entry.sha256 === candidate.detail.artifact.sha256);
        if (version) {
          candidate.assetVersionId = version.id;
          candidate.versionNumber = version.versionNumber;
        }
      }
    } catch {
      // The generated candidate remains reviewable even if its imported
      // AssetVersion projection is temporarily unavailable.
    }
    return { detail, artifacts: candidates };
  }

  function handleRunChange(next: WorkflowRunDetail) {
    if (next.run.status === "completed") {
      void resolveRun(next).then((resolved) => {
        setRuns((current) => current.map((run) => run.detail.run.id === next.run.id ? resolved : run));
      });
      return;
    }
    setRuns((current) => current.map((run) => run.detail.run.id === next.run.id ? { ...run, detail: next } : run));
  }

  async function handleUseForShot(artifactId: string) {
    setPromoting(artifactId);
    setError(null);
    try {
      await promoteShotVideoCandidate(
        projectRootPath,
        shot.id,
        artifactId,
        shot.generatedVideoAssetVersionId,
      );
      onShotChanged();
    } catch (caught: unknown) {
      setError(describeError(caught));
    } finally {
      setPromoting(null);
    }
  }

  const runActive = runs.some(isActiveRun);
  const durationSeconds = parseDurationSeconds(duration);
  const disabledReason = !source
    ? "Add or generate a keyframe first."
    : !prompt.trim()
      ? "Add a motion description first."
      : durationSeconds === null
        ? "Duration must be between 0.5 and 30 seconds."
        : null;
  const generationDisabled = Boolean(
    disabledReason || creating || runActive || !selection.providerId || !selection.modelId,
  );

  return (
    <div className="shot-i2v" style={{ display: "grid", gap: "var(--space-8)", marginTop: "var(--space-8)" }}>
      <h4 style={{ margin: 0, fontSize: "var(--fs-md)" }}>Animate this shot</h4>

      {shot.generatedVideoAssetVersionId ? (
        <p role="status" style={{ margin: 0, color: "var(--c-muted)" }}>
          Pinned as the current Shot video: {shot.generatedVideoAssetVersionId}
        </p>
      ) : null}

      {sourceMissing || !source ? (
        <p style={{ margin: 0, color: "var(--c-muted)" }}>Add or generate a keyframe first.</p>
      ) : (
        <figure style={{ margin: 0, display: "flex", gap: "var(--space-8)", alignItems: "center" }}>
          {source.thumbnailPath ? (
            <img
              src={convertFileSrc(joinProjectRelativePath(projectRootPath, source.thumbnailPath))}
              alt={`Keyframe version V${String(source.versionNumber).padStart(2, "0")}`}
              width={96}
              height={54}
              style={{ objectFit: "cover", borderRadius: "var(--radius-sm)" }}
            />
          ) : null}
          <figcaption style={{ fontSize: "var(--fs-md)", color: "var(--c-muted)" }}>
            Animates keyframe V{String(source.versionNumber).padStart(2, "0")} — the exact pinned source.
          </figcaption>
        </figure>
      )}

      {!sourceMissing ? (
        <>
          <ProviderModelFields
            projectRootPath={projectRootPath}
            value={selection}
            mediaType="video"
            requiresReferences={true}
            requiredOperation="video.imageToVideo"
            onChange={setSelection}
          />
          <label htmlFor={`shot-i2v-prompt-${shot.id}`}>
            Motion prompt
            <input
              id={`shot-i2v-prompt-${shot.id}`}
              value={prompt}
              onChange={(event) => setPrompt(event.target.value)}
              placeholder="e.g. Slow push-in, console lights flicker"
            />
          </label>
          <label htmlFor={`shot-i2v-duration-${shot.id}`}>
            Duration (s)
            <input
              id={`shot-i2v-duration-${shot.id}`}
              type="number"
              min="0.5"
              max="30"
              step="0.5"
              value={duration}
              onChange={(event) => setDuration(event.target.value)}
            />
          </label>
        </>
      ) : null}

      {disabledReason && disabledReason !== "Add or generate a keyframe first." ? (
        <p role="status" style={{ margin: 0, color: "var(--c-muted)" }}>{disabledReason}</p>
      ) : null}

      <button type="button" onClick={() => void handleGenerate()} disabled={generationDisabled}>
        {creating ? "Generating…" : "Generate Video"}
      </button>

      {error ? <p role="alert">{error}</p> : null}

      {runs.map((run) => (
        run.detail.run.status === "completed" ? (
          <div key={run.detail.run.id} style={{ display: "grid", gap: "var(--space-8)" }}>
            {run.artifacts.length === 0 ? (
              <p role="status" style={{ margin: 0, color: "var(--c-muted)" }}>
                The run completed but produced no reviewable candidates.
              </p>
            ) : (
              run.artifacts.map((candidate) => (
                  <div key={candidate.detail.artifact.id} style={{ display: "grid", gap: "var(--space-4)" }}>
                    <GenerationResultCard
                      projectRootPath={projectRootPath}
                      detail={candidate.detail}
                      selected={false}
                      onSelect={() => {}}
                    />
                    <div style={{ display: "flex", gap: "var(--space-8)", alignItems: "center" }}>
                      <button
                        type="button"
                        onClick={() => void handleUseForShot(candidate.detail.artifact.id)}
                        disabled={promoting !== null}
                      >
                        {promoting === candidate.detail.artifact.id ? "Pinning…" : "Use for Shot"}
                      </button>
                      {shot.generatedVideoAssetVersionId ? (
                        <span style={{ fontSize: "var(--fs-md)", color: "var(--c-muted)" }}>
                          The video pinned as the current Shot video: {shot.generatedVideoAssetVersionId}
                        </span>
                      ) : null}
                    </div>
                    {candidate.assetVersionId && candidate.versionNumber !== null ? (
                      <VideoQaPanel
                        projectRootPath={projectRootPath}
                        assetVersionId={candidate.assetVersionId}
                        versionLabel={`candidate V${String(candidate.versionNumber).padStart(2, "0")}`}
                      />
                    ) : null}
                  </div>
              ))
            )}
          </div>
        ) : (
          <WorkflowRunView key={run.detail.run.id} projectRootPath={projectRootPath} detail={run.detail} onChange={handleRunChange} />
        )
      ))}
    </div>
  );
}
