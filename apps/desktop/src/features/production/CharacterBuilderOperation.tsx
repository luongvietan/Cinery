import { type FormEvent, useState } from "react";
import type { AssetSummary, WorkflowCharacterOption } from "@cinematic/domain";
import { describeError } from "../../lib/errors";
import { openPanel } from "../../lib/panelNavigation";
import { ProviderModelFields } from "../providers/ProviderModelFields";
import { suggestVisualSpec } from "../workflows/api";

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
  const [providerSelection, setProviderSelection] = useState({ providerId: "", modelId: "" });
  const [visualSpec, setVisualSpec] = useState<Record<(typeof VISUAL_FIELDS)[number], string>>(
    Object.fromEntries(VISUAL_FIELDS.map((field) => [field, ""])) as Record<(typeof VISUAL_FIELDS)[number], string>,
  );
  const [suggesting, setSuggesting] = useState(false);
  const [suggestError, setSuggestError] = useState<string | null>(null);

  async function handleSuggest() {
    const character = characters.find((candidate) => candidate.id === characterEntityId);
    const characterName = character?.name ?? "";
    if (!characterName.trim()) {
      setSuggestError("Pick a character first, then ask for suggestions.");
      return;
    }
    setSuggesting(true);
    setSuggestError(null);
    try {
      const suggestion = await suggestVisualSpec(projectRootPath, characterName, "");
      setVisualSpec((current) => {
        const next = { ...current };
        for (const field of VISUAL_FIELDS) {
          const value = suggestion[field];
          if (typeof value === "string" && value.trim()) next[field] = value;
        }
        return next;
      });
      const wardrobe = suggestion.baselineWardrobe;
      if (typeof wardrobe === "string" && wardrobe.trim()) setBaselineWardrobe(wardrobe);
    } catch (caught: unknown) {
      setSuggestError(describeError(caught));
    } finally {
      setSuggesting(false);
    }
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    await onSubmit({
      projectRootPath,
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
        <div><span className="production-kicker">Character Builder</span><h2 id="production-face-lock-title">Create Face Lock</h2><p>Describe the character's look, generate candidates, then approve one as the official face reference.</p></div>
      </header>
      {characters.length === 0 ? (
        <div className="workflow-dead-end" role="status">
          <p>You don't have any characters yet. Create one first: give it a name in the Canon tab, then come back here to lock its face.</p>
          <button type="button" onClick={() => openPanel("canon")}>Open Canon</button>
        </div>
      ) : (
      <form className="production-form" onSubmit={handleSubmit}>
        <label>Character<select value={characterEntityId} onChange={(event) => setCharacterEntityId(event.target.value)} required>{characters.map((character) => <option key={character.id} value={character.id}>{character.name}</option>)}</select></label>
        <label>Source reference<select value={sourceAssetVersionId} onChange={(event) => setSourceAssetVersionId(event.target.value)} required disabled={sourceAssets.length === 0} aria-describedby="source-reference-help"><option value="">No canonical face asset available</option>{sourceAssets.map((asset) => <option key={asset.id} value={asset.canonicalVersionId ?? ""}>{asset.label} · v{String(asset.canonicalVersionNumber ?? 0).padStart(3, "0")} · Canonical</option>)}</select></label>
        {sourceAssets.length === 0 ? (
          <p className="production-help" id="source-reference-help" role="note">
            No approved face yet. That's fine for a first Face Lock: leave this as is and describe the look below.
          </p>
        ) : (
          <p className="production-help" id="source-reference-help">The selected approved version is pinned when this run starts.</p>
        )}
        {suggestError ? <p role="alert">{suggestError}</p> : null}
        <button type="button" className="production-secondary" onClick={() => void handleSuggest()} disabled={suggesting}>
          {suggesting ? "Thinking…" : "Suggest with AI"}
        </button>
        <p className="production-help">Fills the fields below from your story context. Edit anything before running.</p>
        <div className="production-form-grid">{VISUAL_FIELDS.map((field) => <label key={field}>{field[0].toUpperCase() + field.slice(1)}<input value={visualSpec[field]} onChange={(event) => setVisualSpec((current) => ({ ...current, [field]: event.target.value }))} required /></label>)}</div>
        <label>Baseline wardrobe<input value={baselineWardrobe} onChange={(event) => setBaselineWardrobe(event.target.value)} required /></label>
        <ProviderModelFields projectRootPath={projectRootPath} value={providerSelection} mediaType="image" requiresReferences onChange={setProviderSelection} />
        <div className="production-form-actions"><button type="submit" disabled={pending}>{pending ? "Preparing…" : "Review Request"}</button><button className="production-secondary" type="button" onClick={onCancel} disabled={pending}>Cancel</button></div>
      </form>
      )}
    </section>
  );
}
