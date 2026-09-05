import { useEffect, useState } from "react";
import type { SequenceFlow } from "@cinematic/domain";
import { updateSequenceBrief } from "./sequenceFlowApi";
import { describeError } from "../../lib/errors";

const ENERGIES = ["composed", "elevated", "kinetic", "violent"] as const;
type Energy = (typeof ENERGIES)[number];

/** Joey's starter credit cap for one ~15-second prompt unit. */
const DEFAULT_CREDIT_CAP = 800;
const INTENT_MAX_CHARS = 1000;
const DURATION_MAX_SECONDS = 120;

interface SequenceBriefProps {
  projectRootPath: string;
  sceneId: string;
  /** The scene's persisted flow, or null before the first brief is locked. */
  flow: SequenceFlow | null;
  onChanged: () => void;
}

/**
 * The human-authored director brief. The creator owns this artifact: the AI
 * co-director may suggest, but only the explicit "Lock brief" action writes.
 */
export function SequenceBrief({
  projectRootPath,
  sceneId,
  flow,
  onChanged,
}: SequenceBriefProps) {
  const [intent, setIntent] = useState("");
  const [energy, setEnergy] = useState<Energy>("elevated");
  const [duration, setDuration] = useState("");
  const [creditCap, setCreditCap] = useState(String(DEFAULT_CREDIT_CAP));
  const [locking, setLocking] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Keep the controlled fields in sync with the persisted flow (scene
  // switches and post-save reloads). Typing never triggers a reload, so a
  // draft is only ever reconciled against saved state.
  useEffect(() => {
    setIntent(flow?.brief.intent ?? "");
    setEnergy(flow?.brief.energy ?? "elevated");
    setDuration(
      flow?.brief.targetDurationSeconds != null
        ? String(flow.brief.targetDurationSeconds)
        : "",
    );
    setCreditCap(String(flow?.brief.creditCap ?? DEFAULT_CREDIT_CAP));
    setError(null);
  }, [sceneId, flow]);

  const trimmedIntent = intent.trim();
  const durationIsOmitted = duration.trim() === "";
  const parsedDuration = Number(duration);
  const parsedCreditCap = Number(creditCap);
  const briefValid =
    trimmedIntent.length > 0 &&
    trimmedIntent.length <= INTENT_MAX_CHARS &&
    (durationIsOmitted ||
      (Number.isFinite(parsedDuration) &&
        parsedDuration > 0 &&
        parsedDuration <= DURATION_MAX_SECONDS)) &&
    Number.isInteger(parsedCreditCap) &&
    parsedCreditCap >= 0;

  async function handleLock() {
    if (!briefValid || locking) return;
    setLocking(true);
    setError(null);
    try {
      await updateSequenceBrief(projectRootPath, sceneId, {
        intent: trimmedIntent,
        energy,
        targetDurationSeconds: durationIsOmitted ? undefined : parsedDuration,
        creditCap: parsedCreditCap,
      });
      onChanged();
    } catch (caught: unknown) {
      setError(describeError(caught));
    } finally {
      setLocking(false);
    }
  }

  return (
    <section aria-label="Director brief">
      <header style={{ display: "flex", alignItems: "baseline", gap: "var(--space-8)" }}>
        <h3>Director brief</h3>
        {flow ? <span className="scene-status-chip">{flow.stage}</span> : <span className="scene-status-chip">draft</span>}
      </header>
      <p>
        Write what this sequence must feel like. You own this brief — the AI co-director
        can only suggest, never write or lock it.
      </p>
      {error ? <p role="alert">{error}</p> : null}
      <div className="canon-field-grid">
        <label htmlFor="brief-intent">
          Creative intent
          <textarea
            id="brief-intent"
            value={intent}
            onChange={(event) => setIntent(event.target.value)}
            rows={3}
            maxLength={INTENT_MAX_CHARS}
            placeholder="What must this sequence feel like?"
          />
        </label>
        <label htmlFor="brief-energy">
          Energy
          <select
            id="brief-energy"
            value={energy}
            onChange={(event) => setEnergy(event.target.value as Energy)}
          >
            {ENERGIES.map((value) => (
              <option key={value} value={value}>
                {value}
              </option>
            ))}
          </select>
        </label>
        <label htmlFor="brief-duration">
          Target duration (seconds, optional)
          <input
            id="brief-duration"
            type="number"
            min="1"
            max={DURATION_MAX_SECONDS}
            step="0.5"
            value={duration}
            onChange={(event) => setDuration(event.target.value)}
            placeholder="e.g. 15"
          />
        </label>
        <label htmlFor="brief-credit-cap">
          Credit cap
          <input
            id="brief-credit-cap"
            type="number"
            min="0"
            step="1"
            value={creditCap}
            onChange={(event) => setCreditCap(event.target.value)}
          />
        </label>
      </div>
      <button type="button" onClick={() => void handleLock()} disabled={!briefValid || locking}>
        {locking ? "Locking…" : "Lock brief"}
      </button>
    </section>
  );
}
