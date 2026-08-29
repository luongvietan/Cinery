import { type FormEvent, useState } from "react";
import type { WorkflowCharacterOption } from "@cinematic/domain";
import { ProviderModelFields } from "../providers/ProviderModelFields";

interface CreateCharacterSheetFormProps {
  projectRootPath: string;
  characters: WorkflowCharacterOption[];
  pending: boolean;
  onCancel: () => void;
  onSubmit: (input: Record<string, unknown>) => Promise<void>;
}

export function CreateCharacterSheetForm({ projectRootPath, characters, pending, onCancel, onSubmit }: CreateCharacterSheetFormProps) {
  const [characterEntityId, setCharacterEntityId] = useState(characters[0]?.id ?? "");
  const [providerSelection, setProviderSelection] = useState({ providerId: "", modelId: "" });

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    await onSubmit({ projectRootPath, characterEntityId, providerId: providerSelection.providerId, modelId: providerSelection.modelId });
  }

  return (
    <section className="workflow-editor" aria-labelledby="sheet-form-title">
      <header className="workflow-panel-header">
        <div>
          <h2 id="sheet-form-title">Generate character sheet</h2>
          <p>A full-body reference sheet built from the approved outfit, giving every scene an exact view of the character.</p>
        </div>
      </header>
      <form className="workflow-form" onSubmit={handleSubmit}>
        <label htmlFor="sheet-character-id">Character</label>
        <select autoFocus id="sheet-character-id" value={characterEntityId} onChange={(event) => setCharacterEntityId(event.target.value)} required>
          {characters.length === 0 ? <option value="">No Character Canon entities available</option> : characters.map((character) => <option key={character.id} value={character.id}>{character.name}</option>)}
        </select>
        <ProviderModelFields projectRootPath={projectRootPath} value={providerSelection} mediaType="image" requiresReferences onChange={setProviderSelection} />
        <div className="workflow-form-actions">
          <button type="submit" disabled={pending}>{pending ? "Starting…" : "Generate character sheet"}</button>
          <button type="button" onClick={onCancel} disabled={pending}>Cancel</button>
        </div>
      </form>
    </section>
  );
}
