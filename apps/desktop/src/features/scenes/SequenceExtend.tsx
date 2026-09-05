import { useEffect, useState } from "react";
import type { ExtensionDirection, ExtensionRequest, SequenceFlow } from "@cinematic/domain";
import { prepareSequenceExtension } from "./sequenceFlowApi";
import { resolveCanonicalShotVideo } from "./api";
import { describeError } from "../../lib/errors";

interface SequenceExtendProps {
  projectRootPath: string;
  sceneId: string;
  /** The scene's flow; `canonicalShotId` is the only allowed extension source. */
  flow: SequenceFlow | null;
  onChanged: () => void;
}

/**
 * Extension preparation, not execution: lists the exact canonical source
 * version, requires a deliberate direction, and produces an inspectable
 * request. No provider call is ever made from here — the prepared request
 * becomes the explicit input for a future provider-level Extend Video
 * capability, and no credits are spent by preparing it.
 */
export function SequenceExtend({ projectRootPath, sceneId, flow, onChanged }: SequenceExtendProps) {
  const canonicalShotId = flow?.canonicalShotId ?? null;
  const [canonicalVersionId, setCanonicalVersionId] = useState<string | null>(null);
  const [direction, setDirection] = useState<ExtensionDirection | null>(null);
  const [prepared, setPrepared] = useState<ExtensionRequest | null>(null);
  const [preparing, setPreparing] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!canonicalShotId) {
      setCanonicalVersionId(null);
      return;
    }
    let cancelled = false;
    resolveCanonicalShotVideo(projectRootPath, canonicalShotId)
      .then((version) => {
        if (!cancelled) setCanonicalVersionId(version);
      })
      .catch(() => {
        if (!cancelled) setCanonicalVersionId(null);
      });
    return () => {
      cancelled = true;
    };
  }, [projectRootPath, canonicalShotId]);

  const canPrepare = Boolean(canonicalShotId && canonicalVersionId && direction);

  async function handlePrepare() {
    if (!canPrepare || preparing) return;
    setPreparing(true);
    setError(null);
    try {
      const request = await prepareSequenceExtension(
        projectRootPath,
        sceneId,
        direction as ExtensionDirection,
      );
      setPrepared(request);
      onChanged();
    } catch (caught: unknown) {
      setError(describeError(caught));
    } finally {
      setPreparing(false);
    }
  }

  return (
    <section aria-label="Extend canonical take">
      <h3>Extend the canonical take</h3>
      {canonicalShotId && canonicalVersionId ? (
        <p>
          Canonical source: shot {canonicalShotId} — exact version {canonicalVersionId}.
          Extensions always start from this exact pin.
        </p>
      ) : (
        <p>
          Promote a shot's take as canonical first. Extensions only start from the exact
          canonical video of a shot.
        </p>
      )}
      <fieldset style={{ display: "flex", gap: "var(--space-16)", margin: "var(--space-8) 0" }}>
        <legend>Continuation direction</legend>
        <label style={{ display: "flex", gap: "var(--space-4)", alignItems: "center" }}>
          <input
            type="radio"
            name="extension-direction"
            checked={direction === "prequel"}
            onChange={() => setDirection("prequel")}
          />
          Before this clip (prequel)
        </label>
        <label style={{ display: "flex", gap: "var(--space-4)", alignItems: "center" }}>
          <input
            type="radio"
            name="extension-direction"
            checked={direction === "sequel"}
            onChange={() => setDirection("sequel")}
          />
          After this clip (sequel)
        </label>
      </fieldset>
      {error ? <p role="alert">{error}</p> : null}
      {prepared ? (
        <div style={{ margin: "var(--space-8) 0", padding: "var(--space-12)", background: "var(--c-panel-soft)", border: "1px solid var(--c-hairline)", borderRadius: "var(--radius-md)" }}>
          <p style={{ margin: "0 0 var(--space-4)", fontWeight: 600 }}>
            Prepared request — nothing has been generated and no credits spent:
          </p>
          <p style={{ margin: "0 0 var(--space-4)" }}>{prepared.continuationPrompt}</p>
          <p style={{ margin: 0, fontSize: "var(--fs-md)" }}>
            Canonical video: {prepared.canonicalVideoAssetVersionId} · Direction: {prepared.direction}
            {prepared.carriedLocks.speech ? ` · Speech: ${prepared.carriedLocks.speech}` : ""}
            {prepared.carriedLocks.movement ? ` · Movement: ${prepared.carriedLocks.movement}` : ""}
            {prepared.carriedLocks.stillness ? ` · Stillness: ${prepared.carriedLocks.stillness}` : ""}
            {prepared.worldContinuity.plateAssetVersionId ? ` · World plate: ${prepared.worldContinuity.plateAssetVersionId}` : ""}
          </p>
        </div>
      ) : null}
      <button type="button" onClick={() => void handlePrepare()} disabled={!canPrepare || preparing}>
        {preparing ? "Preparing…" : "Prepare extension"}
      </button>
      <p style={{ margin: "var(--space-4) 0 0", fontSize: "var(--fs-md)", color: "var(--c-muted)" }}>
        Preparing only stages the request for review. A provider-level Extend Video capability
        will consume this prepared request; nothing runs automatically.
      </p>
    </section>
  );
}
