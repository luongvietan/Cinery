import { useState } from "react";
import { describeError } from "../../lib/errors";
import { ThinkingIndicator } from "../../components/ThinkingIndicator";
import type { RouteProductionIntentResult } from "@cinematic/domain";
import { routeProductionIntent } from "./api";

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
        <span className="production-kicker">AI Director</span>
        <input
          type="text"
          value={text}
          onChange={(event) => setText(event.target.value)}
          placeholder="Describe what you want to produceΓÇª e.g. Put Mara in a raincoat."
          aria-label="Production intent"
          onKeyDown={(event) => { if (event.key === "Enter") { event.preventDefault(); void handleRoute(); } }}
        />
        <button type="button" onClick={() => void handleRoute()} disabled={pending || !text.trim()}>
          {pending ? (
            <span className="ai-director-pending" aria-hidden="true">
              <ThinkingIndicator state="working" />
            </span>
          ) : null}
          {pending ? "RoutingΓÇª" : "Route"}
        </button>
      </div>
      {error ? <p role="alert">{error}</p> : null}
      {result ? (
        <ul className="ai-director-suggestions">
          {result.suggested ? (
            <li className="ai-director-suggestion ai-director-suggestion--primary">
              <span className="ai-director-match">{result.candidates[0]?.score ?? 0}%</span>
              <span>
                <strong>{result.suggested.operationName}</strong>
                <code>{result.suggested.operationId}</code>
              </span>
              {result.suggested.prerequisitePassed ? (
                <span className="ai-director-ready">Ready</span>
              ) : (
                <span className="ai-director-blocked">Blocked</span>
              )}
            </li>
          ) : (
            <li>No production operation matched this intent.</li>
          )}
          {result.candidates.slice(1).map((candidate) => (
            <li key={candidate.operationId} className="ai-director-suggestion">
              <span className="ai-director-match">{candidate.score}%</span>
              <span>
                <strong>{candidate.operationName}</strong>
                <code>{candidate.operationId}</code>
              </span>
              {candidate.prerequisitePassed ? (
                <span className="ai-director-ready">Ready</span>
              ) : (
                <span className="ai-director-blocked" title={candidate.prerequisiteBlockers.join("; ")}>Blocked</span>
              )}
            </li>
          ))}
        </ul>
      ) : null}
    </section>
  );
}
