import type { SequenceFlow } from "@cinematic/domain";

/** Which stage canvas the creator is currently looking at. */
export type SequenceStageHint = "brief" | "references" | "preflight" | "review" | "extension";

interface AiCoDirectorRailProps {
  flow: SequenceFlow | null;
  readiness: { ready: boolean; blockers: Array<{ message: string }> } | null;
  activeStage: SequenceStageHint;
}

/**
 * The persistent, strictly non-autonomous AI co-director rail: it derives a
 * short checklist from the current flow state and suggests the next
 * deliberate step. It never mutates anything — no generate, approve, or
 * promote controls exist here; navigation suggestions are the creator's
 * own actions rendered elsewhere.
 */
export function AiCoDirectorRail({ flow, readiness, activeStage }: AiCoDirectorRailProps) {
  const items: string[] = [];

  if (!flow) {
    items.push("Lock a director brief that states the intent, energy, duration, and credit cap.");
  }
  if (readiness && !readiness.ready) {
    const first = readiness.blockers[0]?.message ?? "resolve the scene's readiness blockers";
    items.push(`Resolve scene references — ${first}`);
  }
  if (flow) {
    switch (flow.stage) {
      case "draft":
      case "brief_locked":
        items.push("Mark references ready once the world plate, cast looks, and shots are final.");
        break;
      case "references_ready":
        items.push("Read the generation preflight, then approve it deliberately.");
        break;
      case "prompt_approved":
      case "generating":
        items.push("Let the run finish, then review every candidate before promoting.");
        break;
      case "in_review":
        items.push("Promote one take as the canonical video for its shot.");
        break;
      case "canonical_selected":
        items.push("Choose a prequel or sequel direction to extend the canonical take.");
        break;
      case "ready_for_edit":
        break;
    }
  }

  const suggestions = items.slice(0, 3);

  return (
    <section aria-label="AI co-director">
      <header style={{ display: "flex", alignItems: "baseline", gap: "var(--space-8)" }}>
        <h3>AI co-director</h3>
        <span className="scene-status-chip">{activeStage}</span>
      </header>
      <p style={{ fontSize: "var(--fs-md)", color: "var(--c-muted)" }}>
        Suggestions only — nothing here generates, approves, or spends credits.
      </p>
      {suggestions.length ? (
        <ul style={{ margin: 0, paddingLeft: "var(--space-20)" }}>
          {suggestions.map((item) => (
            <li key={item} style={{ marginBottom: "var(--space-4)" }}>
              {item}
            </li>
          ))}
        </ul>
      ) : (
        <p style={{ fontSize: "var(--fs-md)" }}>This sequence is ready for the edit bay.</p>
      )}
    </section>
  );
}
