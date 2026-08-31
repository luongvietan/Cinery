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

interface ShotImageToVideoProps {
  projectRootPath: string;
  sceneId: string;
  shot: Shot;
  onShotChanged(): void;
}

interface ResultState {
  detail: WorkflowRunDetail;
  artifacts: GeneratedArtifactDetail[];
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
  const [run, setRun] = useState<ResultState | null>(null);
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

  // Durable status restoration: remount re-attaches to this Shot's latest
  // persisted I2V run through WorkflowRunView. No local timer — observation
  // is centralized. A transient read failure keeps the previous detail.
  useEffect(() => {
    let cancelled = false;
    listWorkflowRuns(projectRootPath)
      .then((records) => {
        if (cancelled) return null;
        const latest = records
          .filter((record) => record.operationId === "shot.image_to_video")
          .filter((record) => {
            try {
              const parsed = JSON.parse(record.inputJson) as Record<string, unknown>;
              return parsed.shotId === shot.id;
            } catch {
              return false;
            }
          })
          .sort((a, b) => b.createdAt.localeCompare(a.createdAt))[0];
        return latest ? getWorkflowRun(projectRootPath, latest.id) : null;
      })
      .then((detail) => {
        if (!cancelled && detail) setRun({ detail, artifacts: [] });
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
    // or a run is already being reviewed (state), further clicks never
    // create a second payload.
    if (creatingRef.current || run !== null || !source || !selection.providerId || !selection.modelId) return;
    creatingRef.current = true;
    setCreating(true);
    setError(null);
    try {
      const created = await createWorkflowRun(projectRootPath, "scene-builder", "1.0.0", "shot.image_to_video", {
        sceneId,
        shotId: shot.id,
        providerId: selection.providerId,
        modelId: selection.modelId,
        prompt: prompt.trim(),
        generationParameters: { durationSeconds: Number(duration) || shot.durationSeconds },
      });
      const waiting = await advanceWorkflowRun(projectRootPath, created.run.id);
      await applyRun(waiting);
    } catch (caught: unknown) {
      setError(describeError(caught));
    } finally {
      resetCreateGuard();
    }
  }

  /** Resolve candidates once a run completes so the user reviews actual
   * videos before pinning. */
  async function applyRun(detail: WorkflowRunDetail) {
    if (detail.run.status !== "completed") {
      setRun({ detail, artifacts: [] });
      return;
    }
    let artifacts: GeneratedArtifactDetail[] = [];
    try {
      const resultSets = await listGenerationResults(projectRootPath, detail.run.id);
      artifacts = resultSets.flatMap((resultSet: GenerationResultSetDetail) => resultSet.artifacts);
    } catch {
      // The run view still shows the completed run without candidates.
    }
    setRun({ detail, artifacts });
  }

  function handleRunChange(next: WorkflowRunDetail) {
    if (!run) return;
    if (next.run.status === "completed") {
      void applyRun(next);
      return;
    }
    setRun({ ...run, detail: next });
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
      setRun(null);
      onShotChanged();
    } catch (caught: unknown) {
      setError(describeError(caught));
    } finally {
      setPromoting(null);
    }
  }

  const runActive = run !== null && run.detail.run.status !== "completed";

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
            requiresReferences={false}
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

      <button type="button" onClick={() => void handleGenerate()} disabled={!source || creating || !selection.providerId || !selection.modelId}>
        {creating ? "Generating…" : "Generate Video"}
      </button>

      {error ? <p role="alert">{error}</p> : null}

      {run ? (
        runActive ? (
          <WorkflowRunView projectRootPath={projectRootPath} detail={run.detail} onChange={handleRunChange} />
        ) : (
          <div style={{ display: "grid", gap: "var(--space-8)" }}>
            {run.artifacts.length === 0 ? (
              <p role="status" style={{ margin: 0, color: "var(--c-muted)" }}>
                The run completed but produced no reviewable candidates.
              </p>
            ) : (
              run.artifacts.map((detail) => (
                  <div key={detail.artifact.id} style={{ display: "grid", gap: "var(--space-4)" }}>
                    <GenerationResultCard
                      projectRootPath={projectRootPath}
                      detail={detail}
                      selected={false}
                      onSelect={() => {}}
                    />
                    <div style={{ display: "flex", gap: "var(--space-8)", alignItems: "center" }}>
                      <button
                        type="button"
                        onClick={() => void handleUseForShot(detail.artifact.id)}
                        disabled={promoting !== null}
                      >
                        {promoting === detail.artifact.id ? "Pinning…" : "Use for Shot"}
                      </button>
                      {shot.generatedVideoAssetVersionId ? (
                        <span style={{ fontSize: "var(--fs-md)", color: "var(--c-muted)" }}>
                          The video pinned as the current Shot video: {shot.generatedVideoAssetVersionId}
                        </span>
                      ) : null}
                    </div>
                  </div>
              ))
            )}
          </div>
        )
      ) : null}
    </div>
  );
}
