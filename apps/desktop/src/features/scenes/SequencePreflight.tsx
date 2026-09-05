import { useState } from "react";
import type { SequenceFlow, SequencePreflight as SequencePreflightData } from "@cinematic/domain";
import { approveSequencePreflight, markSequenceReferencesReady } from "./sequenceFlowApi";
import { describeError } from "../../lib/errors";

interface SequencePreflightProps {
  projectRootPath: string;
  sceneId: string;
  /** The scene's flow; its stage tells whether approval already happened. */
  flow: SequenceFlow | null;
  preflight: SequencePreflightData;
  onChanged: () => void;
}

/**
 * Read-only generation disclosure: the complete compiled prompt, the exact
 * resolved references, runtime guidance, estimated credit impact, and every
 * blocker. Nothing is sent until the creator presses the single explicit
 * approval control; a blocked preflight can never be approved.
 */
export function SequencePreflight({
  projectRootPath,
  sceneId,
  flow,
  preflight,
  onChanged,
}: SequencePreflightProps) {
  const [approving, setApproving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [markingReferencesReady, setMarkingReferencesReady] = useState(false);
  const [referenceBlockers, setReferenceBlockers] = useState<Array<{ code: string; message: string }>>([]);

  const approvalAlreadyRecorded =
    flow?.stage === "prompt_approved" ||
    flow?.stage === "generating" ||
    flow?.stage === "in_review" ||
    flow?.stage === "canonical_selected" ||
    flow?.stage === "ready_for_edit";

  // The brief must be locked before references can be marked ready; the
  // approval command itself only accepts the adjacent references_ready ->
  // prompt_approved move, so this explicit step is required first.
  const needsReferencesReady = flow?.stage === "draft" || flow?.stage === "brief_locked";

  async function handleMarkReferencesReady() {
    if (!needsReferencesReady || markingReferencesReady) return;
    setMarkingReferencesReady(true);
    setError(null);
    setReferenceBlockers([]);
    try {
      const report = await markSequenceReferencesReady(projectRootPath, sceneId);
      if (report.blockers.length) {
        setReferenceBlockers(report.blockers);
      } else {
        onChanged();
      }
    } catch (caught: unknown) {
      setError(describeError(caught));
    } finally {
      setMarkingReferencesReady(false);
    }
  }

  async function handleApprove() {
    if (!preflight.canGenerate || approvalAlreadyRecorded || approving) return;
    setApproving(true);
    setError(null);
    try {
      await approveSequencePreflight(
        projectRootPath,
        sceneId,
        preflight.compilation.id || null,
      );
      onChanged();
    } catch (caught: unknown) {
      setError(describeError(caught));
    } finally {
      setApproving(false);
    }
  }

  return (
    <section aria-label="Generation preflight">
      <header style={{ display: "flex", alignItems: "baseline", gap: "var(--space-8)" }}>
        <h3>Generation preflight</h3>
        {approvalAlreadyRecorded ? (
          <span className="scene-status-chip scene-status-chip--ready">Generation approved</span>
        ) : null}
      </header>
      <p>
        Everything below is exactly what will be sent. Nothing is generated — and no credits
        are spent — until you approve it.
      </p>
      {error ? <p role="alert">{error}</p> : null}

      {needsReferencesReady ? (
        <div style={{ margin: "var(--space-8) 0" }}>
          {referenceBlockers.length ? (
            <div role="status">
              <p style={{ margin: "0 0 var(--space-4)", fontWeight: 600 }}>Not ready yet:</p>
              <ul style={{ margin: 0, paddingLeft: "var(--space-20)" }}>
                {referenceBlockers.map((blocker) => (
                  <li key={blocker.code}>{blocker.message}</li>
                ))}
              </ul>
            </div>
          ) : null}
          <button
            type="button"
            onClick={() => void handleMarkReferencesReady()}
            disabled={markingReferencesReady}
          >
            {markingReferencesReady ? "Checking references…" : "Mark references ready"}
          </button>
        </div>
      ) : null}

      <dl style={{ margin: 0 }}>
        <dt style={{ fontWeight: 600 }}>Prompt (provider-neutral — the service is chosen below)</dt>
        <dd style={{ whiteSpace: "pre-wrap", margin: "var(--space-4) 0 var(--space-8)" }}>
          {preflight.providerPrompt || "(no compiled prompt yet)"}
        </dd>
        <dt style={{ fontWeight: 600 }}>References (exact versions)</dt>
        <dd style={{ margin: "var(--space-4) 0 var(--space-8)" }}>
          {preflight.references.length ? (
            <ul style={{ margin: 0, paddingLeft: "var(--space-20)" }}>
              {preflight.references.map((reference) => (
                <li key={`${reference.role}-${reference.versionId}`}>
                  {reference.role} — version {reference.versionId}
                </li>
              ))}
            </ul>
          ) : (
            "None resolved"
          )}
        </dd>
        <dt style={{ fontWeight: 600 }}>Total runtime</dt>
        <dd style={{ margin: "var(--space-4) 0 var(--space-8)" }}>
          {preflight.compilation.totalDurationSeconds}s — {preflight.runtimeRecommendation}
        </dd>
        <dt style={{ fontWeight: 600 }}>Estimated credits</dt>
        <dd style={{ margin: "var(--space-4) 0 var(--space-8)" }}>
          {preflight.estimatedCredits > 0
            ? preflight.estimatedCredits
            : "not reported until a connected service is selected"}
        </dd>
      </dl>

      {preflight.blockers.length ? (
        <div role="status" style={{ margin: "var(--space-8) 0" }}>
          <p style={{ margin: "0 0 var(--space-4)", fontWeight: 600 }}>Blocked from generation:</p>
          <ul style={{ margin: 0, paddingLeft: "var(--space-20)" }}>
            {preflight.blockers.map((blocker) => (
              <li key={blocker.code}>{blocker.message}</li>
            ))}
          </ul>
        </div>
      ) : null}

      <button
        type="button"
        onClick={() => void handleApprove()}
        disabled={!preflight.canGenerate || approvalAlreadyRecorded || approving}
      >
        {approving ? "Approving…" : "Approve generation"}
      </button>
    </section>
  );
}
