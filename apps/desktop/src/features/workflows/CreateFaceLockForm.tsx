import { type FormEvent, useState } from "react";
import type { WorkflowCharacterOption } from "@cinematic/domain";
import { describeError } from "../../lib/errors";
import { openPanel } from "../../lib/panelNavigation";
import { ProviderModelFields } from "../providers/ProviderModelFields";
import { suggestVisualSpec } from "./api";

interface CreateFaceLockFormProps {
  projectRootPath: string;
  characters: WorkflowCharacterOption[];
  pending: boolean;
  onCancel: () => void;
  onSubmit: (input: Record<string, unknown>) => Promise<void>;
}

const VISUAL_FIELDS = ["head", "eyes", "brows", "nose", "lips", "skin", "hair", "build", "expression"] as const;

export function CreateFaceLockForm({ projectRootPath, characters, pending, onCancel, onSubmit }: CreateFaceLockFormProps) {
  const [characterEntityId, setCharacterEntityId] = useState(characters[0]?.id ?? "");
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
    await onSubmit({ projectRootPath, characterEntityId, visualSpec, baselineWardrobe, providerId: providerSelection.providerId, modelId: providerSelection.modelId });
  }

  return (
    <section className="workflow-editor" aria-labelledby="face-lock-form-title">
      <header className="workflow-panel-header">
        <div>
          <h2 id="face-lock-form-title">Create Face Lock</h2>
          <p>Generate a reference face for a character so they look the same in every scene.</p>
        </div>
      </header>
      {characters.length === 0 ? (
        <div className="workflow-dead-end" role="status">
          <p>You don't have any characters yet. Create one first: give it a name in the Canon tab, then come back here to lock its face.</p>
          <button type="button" onClick={() => openPanel("canon")}>Open Canon</button>
        </div>
      ) : (
      <form className="workflow-form" onSubmit={handleSubmit}>
        <label htmlFor="workflow-character-id">Character</label>
        <select autoFocus id="workflow-character-id" value={characterEntityId} onChange={(event) => setCharacterEntityId(event.target.value)} required aria-describedby="workflow-character-help">
          {characters.map((character) => <option key={character.id} value={character.id}>{character.name}</option>)}
        </select>
        <span id="workflow-character-help" className="workflow-field-help">Only characters from this project are listed.</span>
        {suggestError ? <p role="alert">{suggestError}</p> : null}
        <button type="button" onClick={() => void handleSuggest()} disabled={suggesting}>
          {suggesting ? "Thinking…" : "Suggest with AI"}
        </button>
        <span className="workflow-field-help">Fills the fields below from your story context. Edit anything before running.</span>
        <div className="workflow-form-grid">
          {VISUAL_FIELDS.map((field) => (
            <label key={field}>
              <span>{field[0].toUpperCase() + field.slice(1)}</span>
              <input value={visualSpec[field]} onChange={(event) => setVisualSpec((current) => ({ ...current, [field]: event.target.value }))} required />
            </label>
          ))}
        </div>
        <label htmlFor="workflow-wardrobe">Baseline wardrobe</label>
        <input id="workflow-wardrobe" value={baselineWardrobe} onChange={(event) => setBaselineWardrobe(event.target.value)} required />
        <ProviderModelFields projectRootPath={projectRootPath} value={providerSelection} mediaType="image" requiresReferences onChange={setProviderSelection} />
        <div className="workflow-form-actions">
          <button type="submit" disabled={pending}>Create workflow run</button>
          <button type="button" onClick={onCancel} disabled={pending}>Cancel</button>
        </div>
      </form>
      )}
    </section>
  );
}
