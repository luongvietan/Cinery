import { useEffect, useState } from "react";
import { describeError } from "../../lib/errors";
import {
  getScene,
  listSceneCharacters,
  listSceneTbdBindings,
  setSceneTbdBinding,
  type SceneTbdBindingRecord,
} from "./api";
import { listCanonTbds } from "../canon/api";
import { listWorlds } from "../worlds/api";
import type { CanonTbd } from "@cinematic/domain";
import type { Scene } from "./types";

interface SceneTbdPanelProps {
  projectRootPath: string;
  sceneId: string;
  /** Notified after a decision persists so readiness panels refresh. */
  onDecisionsChanged?: () => void;
}

type DecisionKind = "preserve_unknown" | "not_applicable";

interface LocalDecision {
  tbdId: string;
  topicSnapshot: string;
  noteSnapshot: string | null;
  decision: DecisionKind;
  justification: string | null;
}

export function SceneTbdPanel({ projectRootPath, sceneId, onDecisionsChanged }: SceneTbdPanelProps) {
  const [scene, setScene] = useState<Scene | null>(null);
  const [relevantTbds, setRelevantTbds] = useState<CanonTbd[]>([]);
  const [decisions, setDecisions] = useState<Record<string, LocalDecision>>({});
  const [justifications, setJustifications] = useState<Record<string, string>>({});
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [validationError, setValidationError] = useState<string | null>(null);
  const [savingTbdId, setSavingTbdId] = useState<string | null>(null);
  const [savedTbdIds, setSavedTbdIds] = useState<Set<string>>(new Set());

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    async function load() {
      try {
        const [fetchedScene, allTbds, characters, worlds, bindings] = await Promise.all([
          getScene(projectRootPath, sceneId),
          listCanonTbds(projectRootPath),
          listSceneCharacters(projectRootPath, sceneId).catch(() => []),
          listWorlds(projectRootPath).catch(() => []),
          listSceneTbdBindings(projectRootPath, sceneId).catch(() => [] as SceneTbdBindingRecord[]),
        ]);
        if (cancelled) return;
        setScene(fetchedScene);

        // Determine relevant entity ids: world location + character ids + project-scoped (null)
        const entityIds = new Set<string>();
        if (fetchedScene.worldId) {
          const world = worlds.find((w) => w.id === fetchedScene.worldId);
          if (world) entityIds.add(world.canonLocationEntityId);
        }
        for (const ch of characters) {
          entityIds.add(ch.characterEntityId);
        }

        const relevant = (allTbds ?? []).filter((tbd) => {
          if (!tbd.protected || tbd.status !== "open") return false;
          if (tbd.canonEntityId === null) return true; // project-scoped always relevant
          return entityIds.has(tbd.canonEntityId);
        });
        setRelevantTbds(relevant);

        // Rehydrate persisted decisions so a reload shows saved state.
        const nextDecisions: Record<string, LocalDecision> = {};
        const nextJustifications: Record<string, string> = {};
        const nextSaved = new Set<string>();
        for (const binding of bindings ?? []) {
          nextDecisions[binding.canonTbdId] = {
            tbdId: binding.canonTbdId,
            topicSnapshot: binding.topicSnapshot,
            noteSnapshot: binding.noteSnapshot,
            decision: binding.decision,
            justification: binding.justification,
          };
          if (binding.justification) {
            nextJustifications[binding.canonTbdId] = binding.justification;
          }
          nextSaved.add(binding.canonTbdId);
        }
        setDecisions(nextDecisions);
        setJustifications(nextJustifications);
        setSavedTbdIds(nextSaved);
      } catch (caught: unknown) {
        if (!cancelled) setError(describeError(caught));
      } finally {
        if (!cancelled) setLoading(false);
      }
    }
    void load();
    return () => {
      cancelled = true;
    };
  }, [projectRootPath, sceneId]);

  function isProjectScoped(tbd: CanonTbd): boolean {
    return tbd.canonEntityId === null;
  }

  /** Persists the decision through the command boundary. */
  async function persistDecision(tbd: CanonTbd, kind: DecisionKind, justification: string | null) {
    setSavingTbdId(tbd.id);
    setError(null);
    try {
      await setSceneTbdBinding(projectRootPath, sceneId, tbd.id, kind, justification);
      setSavedTbdIds((prev) => new Set(prev).add(tbd.id));
      onDecisionsChanged?.();
    } catch (caught: unknown) {
      setError(describeError(caught));
    } finally {
      setSavingTbdId(null);
    }
  }

  function handleDecisionChange(tbd: CanonTbd, kind: DecisionKind) {
    setValidationError(null);
    const justification = justifications[tbd.id] ?? "";
    if (kind === "not_applicable" && !isProjectScoped(tbd)) {
      setValidationError(`TBD ${tbd.id} is directly scoped and must be preserve_unknown, not not_applicable`);
      return;
    }
    if (kind === "not_applicable" && !justification.trim()) {
      setValidationError(`TBD ${tbd.id} not_applicable requires a non-empty justification`);
      // still allow setting but show error
    }
    setDecisions((prev) => ({
      ...prev,
      [tbd.id]: {
        tbdId: tbd.id,
        topicSnapshot: tbd.topic,
        noteSnapshot: tbd.note ?? null,
        decision: kind,
        justification: kind === "not_applicable" ? justification.trim() || null : null,
      },
    }));
    // Persist immediately; for not_applicable the justification-update path
    // below re-persists once the required text is present.
    void persistDecision(tbd, kind, kind === "not_applicable" ? justification.trim() || null : null);
  }

  function handleJustificationChange(tbd: CanonTbd, value: string) {
    setJustifications((prev) => ({ ...prev, [tbd.id]: value }));
    setValidationError(null);
    // Update the local decision, and re-persist once a complete
    // not_applicable decision exists (binding requires a justification).
    setDecisions((prev) => {
      const existing = prev[tbd.id];
      if (!existing || existing.decision !== "not_applicable") return prev;
      const next = {
        ...existing,
        justification: value.trim() || null,
      };
      if (value.trim()) {
        void persistDecision(tbd, "not_applicable", value.trim());
      }
      return {
        ...prev,
        [tbd.id]: next,
      };
    });
  }

  if (loading) return <p role="status">Loading TBD decisions…</p>;
  if (error) return <p role="alert">{error}</p>;

  return (
    <section
      aria-label="TBD decisions"
      style={{ padding: "var(--space-16)", background: "var(--c-panel)", border: "1px solid var(--c-hairline)", borderRadius: "var(--radius-lg)" }}
    >
      <header>
        <h3 style={{ margin: 0, textTransform: "uppercase", fontSize: "var(--fs-md)", letterSpacing: "0.04em" }}>TBD DECISIONS</h3>
        <p style={{ marginTop: "var(--space-4)", fontSize: "var(--fs-sm)", color: "var(--c-muted)" }}>
          All relevant protected TBDs must have an explicit handling decision. Directly scoped TBDs must be preserve_unknown. Project-scoped TBDs may be not_applicable with justification.
        </p>
      </header>

      {validationError ? <p role="alert" style={{ marginTop: "var(--space-8)" }}>{validationError}</p> : null}

      {relevantTbds.length === 0 ? (
        <p style={{ marginTop: "var(--space-12)" }}>No protected unknowns for this Scene.</p>
      ) : (
        <ul style={{ listStyle: "none", padding: 0, margin: "var(--space-12) 0 0", display: "flex", flexDirection: "column", gap: "var(--space-12)" }}>
          {relevantTbds.map((tbd) => {
            const isProject = isProjectScoped(tbd);
            const decision = decisions[tbd.id]?.decision ?? null;
            const justification = justifications[tbd.id] ?? decisions[tbd.id]?.justification ?? "";
            return (
              <li
                key={tbd.id}
                style={{ padding: "var(--space-12)", background: "var(--c-panel-soft)", border: "1px solid var(--c-hairline)", borderRadius: "var(--radius-md)" }}
              >
                <div style={{ display: "flex", gap: "var(--space-8)", alignItems: "baseline", flexWrap: "wrap" }}>
                  <strong>{tbd.topic}</strong>
                  <span className="canon-badge">PROTECTED</span>
                  <span style={{ fontSize: "var(--fs-xs)", textTransform: "uppercase", color: "var(--c-muted)" }}>
                    {isProject ? "PROJECT-SCOPED" : `SCOPED TO ${tbd.canonEntityId}`}
                  </span>
                  {savingTbdId === tbd.id ? (
                    <span style={{ fontSize: "var(--fs-xs)", color: "var(--c-muted)" }} role="status">Saving…</span>
                  ) : savedTbdIds.has(tbd.id) && decision ? (
                    <span style={{ fontSize: "var(--fs-xs)", color: "var(--c-muted)" }}>Saved</span>
                  ) : null}
                  {tbd.sectionKey ? <span style={{ fontSize: "var(--fs-xs)" }}>section: {tbd.sectionKey}</span> : null}
                </div>
                {tbd.note ? <p style={{ marginTop: "var(--space-4)", fontSize: "var(--fs-md)" }}>{tbd.note}</p> : null}
                <div style={{ marginTop: "var(--space-8)", display: "flex", gap: "var(--space-12)", alignItems: "center", flexWrap: "wrap" }}>
                  <label style={{ display: "flex", gap: "var(--space-8)", alignItems: "center", fontSize: "var(--fs-md)", fontWeight: 500 }}>
                    <input
                      type="radio"
                      name={`tbd-decision-${tbd.id}`}
                      value="preserve_unknown"
                      checked={decision === "preserve_unknown"}
                      onChange={() => handleDecisionChange(tbd, "preserve_unknown")}
                      aria-label={`Preserve unknown for ${tbd.topic}`}
                    />
                    Preserve Unknown
                  </label>
                  <label style={{ display: "flex", gap: "var(--space-8)", alignItems: "center", fontSize: "var(--fs-md)", fontWeight: 500, opacity: isProject ? 1 : 0.5 }}>
                    <input
                      type="radio"
                      name={`tbd-decision-${tbd.id}`}
                      value="not_applicable"
                      checked={decision === "not_applicable"}
                      onChange={() => handleDecisionChange(tbd, "not_applicable")}
                      disabled={!isProject}
                      aria-label={`Not applicable for ${tbd.topic}`}
                    />
                    Not Applicable
                  </label>
                  {!isProject && decision === "not_applicable" ? (
                    <span style={{ color: "var(--c-danger)", fontSize: "var(--fs-sm)" }}>Must be preserve_unknown for directly scoped TBDs</span>
                  ) : null}
                </div>
                {decision === "not_applicable" ? (
                  <div style={{ marginTop: "var(--space-8)" }}>
                    <label htmlFor={`justification-${tbd.id}`} style={{ fontSize: "var(--fs-sm)", fontWeight: 500, color: "var(--c-muted)" }}>
                      Justification (required for not_applicable)
                    </label>
                    <textarea
                      id={`justification-${tbd.id}`}
                      value={justification}
                      onChange={(event) => handleJustificationChange(tbd, event.target.value)}
                      placeholder="Explain why this global unknown is not applicable to this scene…"
                      rows={2}
                      aria-label={`Justification for ${tbd.topic}`}
                      style={{
                        width: "100%",
                        marginTop: "var(--space-4)",
                        padding: "var(--space-8) var(--space-12)",
                        border: "1px solid var(--c-hairline)",
                        borderRadius: "var(--radius-md)",
                        fontFamily: "inherit",
                      }}
                      required={decision === "not_applicable"}
                    />
                    {decision === "not_applicable" && !justification.trim() ? (
                      <p style={{ color: "var(--c-danger)", fontSize: "var(--fs-sm)", marginTop: "var(--space-4)" }}>Justification is required for not_applicable</p>
                    ) : null}
                  </div>
                ) : null}
                {decision ? (
                  <p style={{ marginTop: "var(--space-8)", fontSize: "var(--fs-xs)", color: "var(--c-muted)", fontFamily: "var(--font-mono)" }}>
                    Snapshot: topic &quot;{tbd.topic}&quot; decision {decision} {justification ? `justification: ${justification}` : ""}
                  </p>
                ) : null}
              </li>
            );
          })}
        </ul>
      )}
      <p style={{ marginTop: "var(--space-8)", fontSize: "var(--fs-sm)", color: "var(--c-muted)" }}>
        Each answer is recorded with the scene at decision time, so later changes never rewrite history.
      </p>
    </section>
  );
}
