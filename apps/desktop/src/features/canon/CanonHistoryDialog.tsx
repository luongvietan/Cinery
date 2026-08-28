import { useEffect, useState } from "react";
import type { CanonSectionRevision } from "@cinematic/domain";
import { describeError } from "../../lib/errors";
import { listCanonSectionRevisions } from "./api";

interface CanonHistoryDialogProps {
  projectRootPath: string;
  sectionId: string | null;
  sectionTitle: string;
  onClose: () => void;
}

export function CanonHistoryDialog({ projectRootPath, sectionId, sectionTitle, onClose }: CanonHistoryDialogProps) {
  const [revisions, setRevisions] = useState<CanonSectionRevision[]>([]);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    if (!sectionId) return;
    listCanonSectionRevisions(projectRootPath, sectionId)
      .then(setRevisions)
      .catch((caught) => setError(describeError(caught)));
  }, [projectRootPath, sectionId]);

  useEffect(() => {
    if (!sectionId) return;
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        event.preventDefault();
        onClose();
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [sectionId, onClose]);

  if (!sectionId) return null;
  return (
    <div className="canon-dialog-backdrop" role="presentation" onClick={onClose}>
      <section className="canon-dialog" role="dialog" aria-modal="true" aria-label={`${sectionTitle} history`} onClick={(event) => event.stopPropagation()}>
        <header><h2>{sectionTitle} history</h2><button type="button" autoFocus onClick={onClose}>Close</button></header>
        {error ? <p role="alert">{error}</p> : null}
        {revisions.length === 0 && !error ? <p role="status">No revisions recorded yet.</p> : null}
        <ol className="canon-history-list">
          {revisions.map((revision) => (
            <li key={revision.id}>
              <strong>Revision {revision.revision}</strong> · {revision.changeKind} · {revision.status}
              {revision.reason ? <p>{revision.reason}</p> : null}
              <time>{revision.createdAt}</time>
              <pre>{JSON.stringify(revision.value, null, 2)}</pre>
            </li>
          ))}
        </ol>
      </section>
    </div>
  );
}
