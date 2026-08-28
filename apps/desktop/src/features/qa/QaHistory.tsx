import type { QaRunRecord } from "./types";

interface QaHistoryProps {
  runs: QaRunRecord[];
  selectedRunId: string | null;
  onSelect: (qaRunId: string) => void;
}

export function QaHistory({ runs, selectedRunId, onSelect }: QaHistoryProps) {
  return (
    <section className="qa-history" aria-label="QA history">
      <h4>QA History</h4>
      {runs.length === 0 ? (
        <p>No QA history for this version.</p>
      ) : (
        <ul>
          {runs.map((run) => (
            <li key={run.id}>
              <button
                type="button"
                className={run.id === selectedRunId ? "qa-history-row qa-history-row--selected" : "qa-history-row"}
                aria-pressed={run.id === selectedRunId}
                onClick={() => onSelect(run.id)}
              >
                <span>{new Date(run.createdAt).toLocaleString()}</span>
                <span>{run.overallStatus?.replace("_", " ") ?? run.status}</span>
                <span>{run.id}</span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
