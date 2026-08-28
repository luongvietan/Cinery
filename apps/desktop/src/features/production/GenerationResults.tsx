import { useState } from "react";
import type { AssetSummary, AssetVersion, GenerationResultSetDetail } from "@cinematic/domain";
import { GenerationResultCard } from "./GenerationResultCard";
import { PromoteArtifactDialog } from "./PromoteArtifactDialog";

export function GenerationResults({ projectRootPath, resultSets, targetAsset, onPromoted }: { projectRootPath: string; resultSets: GenerationResultSetDetail[]; targetAsset: AssetSummary | null; onPromoted: (version: AssetVersion) => void }) {
  const artifacts = resultSets.flatMap((result) => result.artifacts);
  const [selectedId, setSelectedId] = useState(artifacts[0]?.artifact.id ?? "");
  const [promoting, setPromoting] = useState(false);
  const selected = artifacts.find((detail) => detail.artifact.id === selectedId);
  if (!artifacts.length) return null;
  return <section className="generation-results" aria-labelledby="generation-results-title"><header className="production-panel-header"><div><span className="production-kicker">Candidate set</span><h2 id="generation-results-title">Face Lock Results</h2><p>{artifacts.length} candidates generated from the pinned source reference.</p></div></header><div className="generation-result-grid">{artifacts.map((detail) => <GenerationResultCard key={detail.artifact.id} projectRootPath={projectRootPath} detail={detail} selected={detail.artifact.id === selectedId} onSelect={() => setSelectedId(detail.artifact.id)} />)}</div><div className="generation-results-actions"><button type="button" disabled={!selected || !targetAsset} onClick={() => setPromoting(true)}>Save as Asset Version</button></div>{promoting && selected && targetAsset ? <PromoteArtifactDialog projectRootPath={projectRootPath} artifactId={selected.artifact.id} targetAsset={targetAsset} onClose={() => setPromoting(false)} onPromoted={(version) => { setPromoting(false); onPromoted(version); }} /> : null}</section>;
}
