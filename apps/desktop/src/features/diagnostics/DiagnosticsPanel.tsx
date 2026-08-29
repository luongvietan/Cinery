import type { DiagnosticsBundle } from "@cinematic/domain";
import { useEffect, useState } from "react";
import { describeError } from "../../lib/errors";
import { exportDiagnostics, getDiagnosticsFolder } from "./api";
import { JobsPanel } from "../jobs/JobsPanel";

interface Props {
  projectRootPath: string;
}

const FILE_ORDER = [
  "app-version.json",
  "project-summary.json",
  "database-version.json",
  "project-health.json",
  "active-jobs.json",
  "recent-workflows.json",
  "logs.txt",
] as const;

export function DiagnosticsPanel({ projectRootPath }: Props) {
  const [folder, setFolder] = useState<string | null>(null);
  const [bundle, setBundle] = useState<DiagnosticsBundle | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isExporting, setIsExporting] = useState(false);

  useEffect(() => {
    getDiagnosticsFolder(projectRootPath)
      .then(setFolder)
      .catch((reason) => setError(describeError(reason)));
  }, [projectRootPath]);

  async function handleExport() {
    setError(null);
    setIsExporting(true);
    try {
      const next = await exportDiagnostics(projectRootPath);
      setBundle(next);
    } catch (reason) {
      setError(describeError(reason));
    } finally {
      setIsExporting(false);
    }
  }

  const orderedFiles = bundle
    ? [...bundle.files].sort(
        (a, b) =>
          FILE_ORDER.indexOf(a.name as (typeof FILE_ORDER)[number]) -
          FILE_ORDER.indexOf(b.name as (typeof FILE_ORDER)[number]),
      )
    : [];

  return (
    <section className="diagnostics-panel" aria-labelledby="diagnostics-title">
      <header className="workflow-panel-header">
        <div>
          <h2 id="diagnostics-title">Diagnostics</h2>
          <p>
            Export a redacted diagnostics bundle to diagnose this project without
            raw database inspection. No media or credentials are included.
          </p>
        </div>
      </header>

      {error ? <p role="alert" className="diagnostics-error">{error}</p> : null}

      {folder ? (
        <p className="diagnostics-folder">
          Diagnostics folder: <code>{folder}</code>
        </p>
      ) : null}

      <div className="diagnostics-actions">
        <button type="button" onClick={handleExport} disabled={isExporting}>
          {isExporting ? "Exporting…" : "Export diagnostics bundle"}
        </button>
      </div>

      {bundle ? (
        <div className="diagnostics-result">
          <p role="status">
            Bundle <code>{bundle.fileName}</code> exported at {bundle.exportedAt}.
          </p>
          <ul className="diagnostics-file-list">
            {orderedFiles.map((file) => (
              <li key={file.name}>
                <details>
                  <summary>{file.name}</summary>
                  <pre>{file.content}</pre>
                </details>
              </li>
            ))}
          </ul>
        </div>
      ) : null}

      <details className="diagnostics-jobs" style={{ marginTop: "var(--space-16)" }}>
        <summary>Background jobs &amp; recovery</summary>
        <JobsPanel projectRootPath={projectRootPath} />
      </details>
    </section>
  );
}
