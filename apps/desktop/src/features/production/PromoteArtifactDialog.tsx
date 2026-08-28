import { useState } from "react";
import type { AssetSummary, AssetVersion } from "@cinematic/domain";
import { describeError } from "../../lib/errors";
import { promoteGeneratedArtifact } from "./api";

export function PromoteArtifactDialog({ projectRootPath, artifactId, targetAsset, onClose, onPromoted }: { projectRootPath: string; artifactId: string; targetAsset: AssetSummary; onClose: () => void; onPromoted: (version: AssetVersion) => void }) {
  const [setCanonical, setSetCanonical] = useState(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  async function save() {
    setPending(true); setError(null);
    try { onPromoted(await promoteGeneratedArtifact(projectRootPath, artifactId, targetAsset.id, setCanonical)); }
    catch (reason) { setError(describeError(reason)); }
    finally { setPending(false); }
  }
  return <div className="production-dialog-backdrop" role="presentation"><section className="production-dialog" role="dialog" aria-modal="true" aria-labelledby="promotion-title"><header><div><span className="production-kicker">Explicit promotion</span><h2 id="promotion-title">Save Generated Result</h2></div><button className="production-secondary" type="button" onClick={onClose} disabled={pending}>Close</button></header><p><strong>{targetAsset.label}</strong> · current canonical v{String(targetAsset.canonicalVersionNumber ?? 0).padStart(3, "0")}</p>{error ? <p role="alert">{error}</p> : null}<label className="production-checkbox"><input type="checkbox" checked={setCanonical} onChange={(event) => setSetCanonical(event.target.checked)} /> Make the new version canonical</label><div className="production-form-actions"><button type="button" onClick={() => void save()} disabled={pending}>{pending ? "Saving…" : "Save Version"}</button><button className="production-secondary" type="button" onClick={onClose} disabled={pending}>Cancel</button></div></section></div>;
}
