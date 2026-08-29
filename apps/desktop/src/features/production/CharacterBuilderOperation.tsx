import { type FormEvent, useState } from "react";
import type { AssetSummary, WorkflowCharacterOption } from "@cinematic/domain";
import { ProviderModelFields } from "../providers/ProviderModelFields";

const VISUAL_FIELDS = ["head", "eyes", "brows", "nose", "lips", "skin", "hair", "build", "expression"] as const;

export function CharacterBuilderOperation({
  projectRootPath,
  characters,
  sourceAssets,
  pending,
  onCancel,
  onSubmit,
}: {
  projectRootPath: string;
  characters: WorkflowCharacterOption[];
  sourceAssets: AssetSummary[];
  pending: boolean;
  onCancel: () => void;
  onSubmit: (input: Record<string, unknown>) => Promise<void>;
}) {
  const [characterEntityId, setCharacterEntityId] = useState(characters[0]?.id ?? "");
  const [sourceAssetVersionId, setSourceAssetVersionId] = useState(sourceAssets[0]?.canonicalVersionId ?? "");
  const [baselineWardrobe, setBaselineWardrobe] = useState("");
  const [providerSelection, setProviderSelection] = useState({ providerId: "mock", modelId: "mock-image-v1" });
  const [visualSpec, setVisualSpec] = useState<Record<(typeof VISUAL_FIELDS)[number], string>>(
    Object.fromEntries(VISUAL_FIELDS.map((field) => [field, ""])) as Record<(typeof VISUAL_FIELDS)[number], string>,
  );

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    await onSubmit({
      characterEntityId,
      sourceAssetVersionId,
      visualSpec,
      baselineWardrobe,
      providerId: providerSelection.providerId,
      modelId: providerSelection.modelId,
    });
  }

  return (
    <section className="production-editor" aria-labelledby="production-face-lock-title">
      <header className="production-panel-header">
        <div><span className="production-kicker">Character Builder</span><h2 id="production-face-lock-title">Create Face Lock</h2><p>Pin a canonical source, review the compiled request, then generate candidates.</p></div>
      </header>
      <form className="production-form" onSubmit={handleSubmit}>
        <label>Character<select value={characterEntityId} onChange={(event) => setCharacterEntityId(event.target.value)} required>{characters.length === 0 ? <option value="">No Character Canon entities available</option> : characters.map((character) => <option key={character.id} value={character.id}>{character.name}</option>)}</select></label>
        <label>Source reference<select value={sourceAssetVersionId} onChange={(event) => setSourceAssetVersionId(event.target.value)} required disabled={sourceAssets.length === 0} aria-describedby="source-reference-help"><option value="">No canonical face asset available</option>{sourceAssets.map((asset) => <option key={asset.id} value={asset.canonicalVersionId ?? ""}>{asset.label} · v{String(asset.canonicalVersionNumber ?? 0).padStart(3, "0")} · Canonical</option>)}</select></label>
        {sourceAssets.length === 0 ? (
          <p className="production-help" id="source-reference-help" role="note">
            Requires a canonical Face. Promote or import a canonical face asset for a character first.
          </p>
        ) : (
          <p className="production-help" id="source-reference-help">The selected AssetVersion is pinned when this production run starts.</p>
        )}
        <div className="production-form-grid">{VISUAL_FIELDS.map((field) => <label key={field}>{field[0].toUpperCase() + field.slice(1)}<input value={visualSpec[field]} onChange={(event) => setVisualSpec((current) => ({ ...current, [field]: event.target.value }))} required /></label>)}</div>
        <label>Baseline wardrobe<input value={baselineWardrobe} onChange={(event) => setBaselineWardrobe(event.target.value)} required /></label>
        <ProviderModelFields projectRootPath={projectRootPath} value={providerSelection} mediaType="image" requiresReferences onChange={setProviderSelection} />
        <div className="production-form-actions"><button type="submit" disabled={pending || !sourceAssetVersionId}>{pending ? "Preparing…" : "Review Request"}</button><button className="production-secondary" type="button" onClick={onCancel} disabled={pending}>Cancel</button></div>
      </form>
    </section>
  );
}
