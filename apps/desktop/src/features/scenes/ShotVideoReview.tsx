import { useCallback, useEffect, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { describeError } from "../../lib/errors";
import {
  listShotVideoCandidates,
  promoteShotVideoCandidate,
  rejectShotVideoCandidate,
  restoreShotVideoCandidate,
  type ShotVideoCandidate,
} from "./api";
import { VideoQaPanel } from "../qa/VideoQaPanel";

interface ShotVideoReviewProps {
  projectRootPath: string;
  shotId: string;
  onChanged(): void;
}

function versionLabel(candidate: ShotVideoCandidate): string {
  return `V${String(candidate.versionNumber).padStart(2, "0")}`;
}

function qaLabel(candidate: ShotVideoCandidate): string {
  if (candidate.qaRunCount === 0) return "QA: not run";
  switch (candidate.qaOverallStatus) {
    case "pass":
      return "QA: passed";
    case "fail":
      return "QA: failed";
    case "needs_review":
      return "QA: needs review";
    default:
      return `QA: ${candidate.qaOverallStatus}`;
  }
}

function exceptionOf(candidate: ShotVideoCandidate): string | null {
  if (candidate.qaOverallStatus === "fail") {
    return "Video QA reported a failure for this candidate.";
  }
  if (candidate.qaOverallStatus === "needs_review") {
    return "Video QA flagged this candidate for review.";
  }
  if (!candidate.sourceKeyframeIsCurrent) {
    return "This video was generated from an earlier keyframe. The Shot's current keyframe has changed since.";
  }
  return null;
}

function formatTimestamp(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime())
    ? value
    : date.toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
}

function formatBytes(byteSize: number): string {
  if (byteSize >= 1_000_000) return `${(byteSize / 1_000_000).toFixed(1)} MB`;
  if (byteSize >= 1_000) return `${Math.round(byteSize / 1_000)} kB`;
  return `${byteSize} B`;
}

/** Human review workspace for a Shot's generated video candidates (P10.4):
 * newest-first gallery, playback, QA + provenance, reject/restore, compare,
 * and explicit canonical promotion with a conflict-safe confirmation. */
export function ShotVideoReview({ projectRootPath, shotId, onChanged }: ShotVideoReviewProps) {
  const [candidates, setCandidates] = useState<ShotVideoCandidate[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [compareId, setCompareId] = useState<string | null>(null);
  const [pendingPromotion, setPendingPromotion] = useState<ShotVideoCandidate | null>(null);
  const [overrideReason, setOverrideReason] = useState("");
  const [confirmOverride, setConfirmOverride] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [conflict, setConflict] = useState<string | null>(null);

  const reload = useCallback(async (): Promise<ShotVideoCandidate[]> => {
    const next = await listShotVideoCandidates(projectRootPath, shotId);
    setCandidates(next);
    return next;
  }, [projectRootPath, shotId]);

  useEffect(() => {
    let cancelled = false;
    listShotVideoCandidates(projectRootPath, shotId)
      .then((next) => {
        if (!cancelled) setCandidates(next);
      })
      .catch((caught: unknown) => {
        if (!cancelled) setError(describeError(caught));
      });
    return () => {
      cancelled = true;
    };
  }, [projectRootPath, shotId]);

  const selected = candidates.find((candidate) => candidate.assetVersionId === selectedId) ?? null;
  const comparison = candidates.find((candidate) => candidate.assetVersionId === compareId) ?? null;

  async function refreshAfterChange() {
    const next = await reload().catch(() => null);
    if (next && selectedId && !next.some((candidate) => candidate.assetVersionId === selectedId)) {
      setSelectedId(null);
      setCompareId(null);
    }
    onChanged();
  }

  function beginPromotion(candidate: ShotVideoCandidate) {
    setError(null);
    setConflict(null);
    setOverrideReason("");
    setConfirmOverride(false);
    setPendingPromotion(candidate);
  }

  async function confirmPromotion() {
    if (!pendingPromotion) return;
    const expected = candidates.find((candidate) => candidate.isCanonical)?.assetVersionId ?? null;
    const exception = exceptionOf(pendingPromotion);
    const needsOverride = exception !== null;
    if (needsOverride && (!confirmOverride || overrideReason.trim() === "")) return;
    setBusy(`promote-${pendingPromotion.assetVersionId}`);
    setError(null);
    setConflict(null);
    try {
      await promoteShotVideoCandidate(
        projectRootPath,
        shotId,
        pendingPromotion.assetVersionId,
        expected,
        needsOverride ? overrideReason.trim() : null,
      );
      setPendingPromotion(null);
      setSelectedId(pendingPromotion.assetVersionId);
      await refreshAfterChange();
    } catch (caught: unknown) {
      const described = describeError(caught);
      // Optimistic concurrency: reload the authoritative state and require
      // another explicit action instead of retrying blindly.
      if (described.toLowerCase().includes("conflict") || described.toLowerCase().includes("changed")) {
        setConflict(
          "The canonical video changed while you were deciding. The latest selection is now shown — please choose again.",
        );
        await reload().catch(() => undefined);
        setPendingPromotion(null);
      } else {
        setError(described);
      }
    } finally {
      setBusy(null);
    }
  }

  async function handleReject(candidate: ShotVideoCandidate) {
    setBusy(`reject-${candidate.assetVersionId}`);
    setError(null);
    setConflict(null);
    try {
      await rejectShotVideoCandidate(projectRootPath, shotId, candidate.assetVersionId, null);
      await refreshAfterChange();
    } catch (caught: unknown) {
      setError(describeError(caught));
    } finally {
      setBusy(null);
    }
  }

  async function handleRestore(candidate: ShotVideoCandidate) {
    setBusy(`restore-${candidate.assetVersionId}`);
    setError(null);
    try {
      await restoreShotVideoCandidate(projectRootPath, shotId, candidate.assetVersionId);
      await refreshAfterChange();
    } catch (caught: unknown) {
      setError(describeError(caught));
    } finally {
      setBusy(null);
    }
  }

  if (candidates.length === 0) {
    return (
      <div className="empty-state" role="status">
        No generated video candidates for this Shot yet.
      </div>
    );
  }

  return (
    <div style={{ display: "grid", gap: "var(--space-8)" }}>
      <h4 style={{ margin: 0, fontSize: "var(--fs-md)" }}>Video candidates</h4>

      {conflict ? <p role="alert">{conflict}</p> : null}
      {error ? <p role="alert">{error}</p> : null}

      <ol className="shot-video-candidates" style={{ listStyle: "none", margin: 0, padding: 0, display: "grid", gap: "var(--space-4)" }}>
        {candidates.map((candidate) => {
          const label = versionLabel(candidate);
          const isSelected = candidate.assetVersionId === selectedId;
          return (
            <li
              key={candidate.assetVersionId}
              style={{
                display: "grid",
                gap: "var(--space-4)",
                padding: "var(--space-4)",
                borderRadius: "var(--radius-sm)",
                border: "1px solid var(--c-border, #333)",
                opacity: candidate.reviewState === "rejected" ? 0.55 : 1,
              }}
            >
              <div style={{ display: "flex", gap: "var(--space-8)", alignItems: "center", flexWrap: "wrap" }}>
                <button
                  type="button"
                  aria-pressed={isSelected}
                  onClick={() => setSelectedId(isSelected ? null : candidate.assetVersionId)}
                  style={{ fontWeight: isSelected ? 700 : 400 }}
                >
                  {label}
                </button>
                {candidate.isCanonical ? (
                  <span
                    data-testid={`canonical-badge-${candidate.assetVersionId}`}
                    className="asset-version-badge asset-version-badge--canonical"
                  >
                    Canonical ✓
                  </span>
                ) : null}
                {candidate.reviewState === "rejected" ? (
                  <span data-testid={`rejected-badge-${candidate.assetVersionId}`}>Rejected</span>
                ) : null}
                <span style={{ color: "var(--c-muted)", fontSize: "var(--fs-md)" }}>{qaLabel(candidate)}</span>
                <span style={{ color: "var(--c-muted)", fontSize: "var(--fs-md)" }}>
                  {formatTimestamp(candidate.createdAt)} · {formatBytes(candidate.byteSize)}
                </span>
              </div>

              <video
                src={convertFileSrc(candidate.filePath)}
                controls
                preload="metadata"
                playsInline
                muted
                aria-label={`Shot video candidate ${label}`}
                style={{ width: "100%", maxHeight: 220 }}
              />

              {isSelected ? (
                <div style={{ display: "grid", gap: "var(--space-4)" }}>
                  <p style={{ margin: 0, color: "var(--c-muted)", fontSize: "var(--fs-md)" }}>
                    {candidate.providerId ?? "unknown provider"} · {candidate.modelId ?? "unknown model"}
                    {candidate.sourceAssetVersionId
                      ? candidate.sourceKeyframeIsCurrent
                        ? " · keyframe current at generation"
                        : " · keyframe changed since generation"
                      : ""}
                  </p>

                  {comparison && comparison.assetVersionId !== candidate.assetVersionId ? (
                    <div
                      data-testid="compare-view"
                      style={{ display: "grid", gap: "var(--space-4)", gridTemplateColumns: "repeat(auto-fit, minmax(240px, 1fr))" }}
                    >
                      <div style={{ display: "grid", gap: "var(--space-4)" }}>
                        <strong>Compare A: {versionLabel(comparison)}</strong>
                        <span>{qaLabel(comparison)}</span>
                        <span>
                          {comparison.isCanonical ? "Canonical ✓" : comparison.reviewState === "rejected" ? "Rejected" : "Noncanonical"}
                        </span>
                        <video
                          src={convertFileSrc(comparison.filePath)}
                          controls
                          preload="metadata"
                          playsInline
                          muted
                          aria-label={`Shot video candidate ${versionLabel(comparison)}`}
                          style={{ width: "100%" }}
                        />
                      </div>
                      <div style={{ display: "grid", gap: "var(--space-4)" }}>
                        <strong>Compare B: {label}</strong>
                        <span>{qaLabel(candidate)}</span>
                        <span>
                          {candidate.isCanonical ? "Canonical ✓" : candidate.reviewState === "rejected" ? "Rejected" : "Noncanonical"}
                        </span>
                        <video
                          src={convertFileSrc(candidate.filePath)}
                          controls
                          preload="metadata"
                          playsInline
                          muted
                          aria-label={`Shot video candidate ${label}`}
                          style={{ width: "100%" }}
                        />
                      </div>
                    </div>
                  ) : null}

                  <div style={{ display: "flex", gap: "var(--space-8)", flexWrap: "wrap" }}>
                    {candidates.length >= 2 ? (
                      <button
                        type="button"
                        onClick={() =>
                          setCompareId(
                            compareId === candidate.assetVersionId
                              ? null
                              : (candidates.find((other) => other.assetVersionId !== candidate.assetVersionId)?.assetVersionId ?? null),
                          )
                        }
                      >
                        {compareId === candidate.assetVersionId ? "Close compare" : "Compare"}
                      </button>
                    ) : null}

                    {candidate.reviewState === "rejected" ? (
                      <button
                        type="button"
                        onClick={() => void handleRestore(candidate)}
                        disabled={busy !== null}
                      >
                        {busy === `restore-${candidate.assetVersionId}` ? "Restoring…" : "Restore"}
                      </button>
                    ) : (
                      <>
                        {candidate.isCanonical ? null : (
                          <button
                            type="button"
                            onClick={() => void handleReject(candidate)}
                            disabled={busy !== null}
                          >
                            {busy === `reject-${candidate.assetVersionId}` ? "Rejecting…" : "Reject"}
                          </button>
                        )}
                        <button
                          type="button"
                          onClick={() => beginPromotion(candidate)}
                          disabled={busy !== null || candidate.isCanonical}
                        >
                          {candidate.isCanonical ? "Canonical ✓" : busy === `promote-${candidate.assetVersionId}` ? "Promoting…" : "Promote"}
                        </button>
                      </>
                    )}
                  </div>

                  {candidate.assetVersionId ? (
                    <VideoQaPanel
                      projectRootPath={projectRootPath}
                      assetVersionId={candidate.assetVersionId}
                      versionLabel={`candidate ${label}`}
                    />
                  ) : null}
                </div>
              ) : null}
            </li>
          );
        })}
      </ol>

      {pendingPromotion ? (
        <div className="production-dialog-backdrop" role="presentation">
          <section
            className="production-dialog"
            role="dialog"
            aria-modal="true"
            aria-labelledby="shot-video-promotion-title"
            aria-describedby="shot-video-promotion-description"
          >
            <h3 id="shot-video-promotion-title" style={{ marginTop: 0 }}>
              Make {versionLabel(pendingPromotion)} the canonical video for this Shot?
            </h3>
            <div id="shot-video-promotion-description" style={{ display: "grid", gap: "var(--space-4)" }}>
              {(() => {
                const current = candidates.find((candidate) => candidate.isCanonical);
                if (!current) {
                  return <p style={{ margin: 0 }}>This Shot has no canonical video yet.</p>;
                }
                if (current.assetVersionId === pendingPromotion.assetVersionId) {
                  return (
                    <p style={{ margin: 0 }}>
                      {versionLabel(pendingPromotion)} is already the canonical video.
                    </p>
                  );
                }
                return (
                  <p style={{ margin: 0 }}>
                    This replaces {versionLabel(current)} as the canonical selection. {versionLabel(current)} will remain available.
                  </p>
                );
              })()}

              {exceptionOf(pendingPromotion) ? (
                <div style={{ display: "grid", gap: "var(--space-4)" }}>
                  <p style={{ margin: 0 }} role="status">
                    {exceptionOf(pendingPromotion)}
                  </p>
                  <p style={{ margin: 0 }}>
                    Promoting it will intentionally preserve this generated result.
                  </p>
                  <label htmlFor="shot-video-override-reason">
                    Why are you promoting this candidate anyway?
                    <textarea
                      id="shot-video-override-reason"
                      value={overrideReason}
                      onChange={(event) => setOverrideReason(event.target.value)}
                      rows={3}
                      placeholder="e.g. Director approved this take despite the QA warning."
                    />
                  </label>
                  <label style={{ display: "flex", gap: "var(--space-4)", alignItems: "center" }}>
                    <input
                      type="checkbox"
                      checked={confirmOverride}
                      onChange={(event) => setConfirmOverride(event.target.checked)}
                    />
                    I understand the reported issue and want to promote anyway
                  </label>
                </div>
              ) : null}
            </div>
            <div style={{ display: "flex", gap: "var(--space-8)", justifyContent: "flex-end", marginTop: "var(--space-8)" }}>
              <button type="button" onClick={() => setPendingPromotion(null)}>
                Cancel
              </button>
              <button
                type="button"
                data-testid="confirm-promotion"
                onClick={() => void confirmPromotion()}
                disabled={
                  busy !== null ||
                  (exceptionOf(pendingPromotion) !== null &&
                    (!confirmOverride || overrideReason.trim() === ""))
                }
              >
                {busy !== null ? "Promoting…" : exceptionOf(pendingPromotion) ? "Promote anyway" : "Promote"}
              </button>
            </div>
          </section>
        </div>
      ) : null}
    </div>
  );
}
