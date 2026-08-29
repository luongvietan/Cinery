import { useState } from "react";
import { describeError } from "../../lib/errors";
import { ThinkingIndicator } from "../../components/ThinkingIndicator";
import type { RouteProductionIntentResult, RoutedOperation } from "@cinematic/domain";
import { openPanelTarget } from "../../lib/panelNavigation";
import { routeProductionIntent } from "./api";

/** Where each kind of generation work lives in the workspace. */
function operationTarget(operationId: string): { panel: "canon" | "worlds" | "scenes" | "assets"; canonTab?: "Characters" } {
  if (operationId.startsWith("character.")) return { panel: "canon", canonTab: "Characters" };
  if (operationId.startsWith("world.")) return { panel: "worlds" };
  if (operationId.startsWith("scene.")) return { panel: "scenes" };
  return { panel: "assets" };
}

function SuggestionRow({ operation, primary }: { operation: RoutedOperation; primary: boolean }) {
  const target = operationTarget(operation.operationId);
  return (
    <li className={primary ? "ai-director-suggestion ai-director-suggestion--primary" : "ai-director-suggestion"}>
      <span className="ai-director-match">{operation.score}%</span>
      <span className="ai-director-name">
        <strong>{operation.operationName}</strong>
      </span>
      {operation.prerequisitePassed ? (
        <button type="button" className="ai-director-open" onClick={() => openPanelTarget(target)}>
          Open
        </button>
      ) : (
        <span className="ai-director-blocked" title={operation.prerequisiteBlockers.join("; ")}>
          Not ready
        </span>
      )}
    </li>
  );
}

/**
 * The Overview intent bar: describe what you want to make and Cinery points
 * at the right place to do it. Suggestions show human operation names and
 * open the workspace where the work happens.
 */
export function AiDirectorBar({ projectRootPath }: { projectRootPath: string }) {
  const [text, setText] = useState("");
  const [result, setResult] = useState<RouteProductionIntentResult | null>(null);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleRoute() {
    const trimmed = text.trim();
    if (!trimmed) return;
    setPending(true); setError(null);
    try {
      setResult(await routeProductionIntent(projectRootPath, trimmed));
    } catch (reason) {
      setError(describeError(reason));
    } finally {
      setPending(false);
    }
  }

  return (
    <section className="ai-director-bar" aria-labelledby="ai-director-title">
      <div className="ai-director-input-row">
        <span className="production-kicker" id="ai-director-title">What do you want to make?</span>
        <input
          type="text"
          value={text}
          onChange={(event) => setText(event.target.value)}
          placeholder="e.g. Put Mara in a raincoat, or generate a rainy street backdrop…"
          aria-label="Describe what you want to produce"
          onKeyDown={(event) => { if (event.key === "Enter") { event.preventDefault(); void handleRoute(); } }}
        />
        <button type="button" onClick={() => void handleRoute()} disabled={pending || !text.trim()}>
          {pending ? (
            <span className="ai-director-pending" aria-hidden="true">
              <ThinkingIndicator state="working" />
            </span>
          ) : null}
          {pending ? "Thinking…" : "Find where"}
        </button>
      </div>
      {error ? <p role="alert">{error}</p> : null}
      {result ? (
        <ul className="ai-director-suggestions">
          {result.suggested ? (
            <SuggestionRow operation={result.suggested} primary />
          ) : (
            <li>No matching tool found. Try naming a character, a world, or a scene.</li>
          )}
          {result.candidates.slice(1).map((candidate) => (
            <SuggestionRow key={candidate.operationId} operation={candidate} primary={false} />
          ))}
        </ul>
      ) : null}
    </section>
  );
}
