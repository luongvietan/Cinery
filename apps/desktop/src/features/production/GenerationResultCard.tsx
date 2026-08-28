import { convertFileSrc } from "@tauri-apps/api/core";
import type { GeneratedArtifactDetail } from "@cinematic/domain";

export function GenerationResultCard({ projectRootPath, detail, selected, onSelect }: { projectRootPath: string; detail: GeneratedArtifactDetail; selected: boolean; onSelect: () => void }) {
  const { artifact, lineage } = detail;
  const source = `${projectRootPath.replace(/[\\/]$/, "")}/${artifact.storagePath}`;
  return (
    <article className={selected ? "generation-result-card generation-result-card--selected" : "generation-result-card"}>
      <button type="button" className="generation-result-select" aria-pressed={selected} aria-label={`Select result ${artifact.ordinal}`} onClick={onSelect}>
        <img src={convertFileSrc(source)} alt={`Generated result ${artifact.ordinal}`} loading="lazy" decoding="async" />
        <span className="generation-result-number">Result {artifact.ordinal}</span>
        <span>{artifact.width ?? "—"} × {artifact.height ?? "—"} · {artifact.mimeType.replace("image/", "").toUpperCase()}</span>
        {selected ? <strong aria-live="polite">Selected</strong> : null}
      </button>
      <details><summary>Generation details</summary><dl><dt>Provider</dt><dd>{lineage?.providerId ?? "—"} · {lineage?.modelId ?? "—"}</dd><dt>Source</dt><dd>{lineage?.sourceAssetVersionIds.join(", ") ?? "—"}</dd><dt>SHA-256</dt><dd>{artifact.sha256}</dd></dl></details>
    </article>
  );
}
