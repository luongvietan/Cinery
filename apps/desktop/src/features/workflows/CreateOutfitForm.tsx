import { type FormEvent, useState } from "react";
import type { WorkflowCharacterOption } from "@cinematic/domain";
import { ProviderModelFields } from "../providers/ProviderModelFields";

interface CreateOutfitFormProps {
  projectRootPath: string;
  characters: WorkflowCharacterOption[];
  pending: boolean;
  onCancel: () => void;
  onSubmit: (input: Record<string, unknown>) => Promise<void>;
}

export function CreateOutfitForm({ projectRootPath, characters, pending, onCancel, onSubmit }: CreateOutfitFormProps) {
  const [characterEntityId, setCharacterEntityId] = useState(characters[0]?.id ?? "");
  const [wardrobeDescription, setWardrobeDescription] = useState("");
  const [providerSelection, setProviderSelection] = useState({ providerId: "mock", modelId: "mock-image-v1" });

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    await onSubmit({
      projectRootPath,
      characterEntityId,
      wardrobeProposal: { description: wardrobeDescription },
      providerId: providerSelection.providerId,
      modelId: providerSelection.modelId,
    });
  }

  return (
    <section className="workflow-editor" aria-labelledby="outfit-form-title">
      <header className="workflow-panel-header">
        <div>
          <h2 id="outfit-form-title">Create Outfit</h2>
          <p>Generates a direct-on-character outfit reference from the canonical face. Requires a canonical face lock.</p>
        </div>
      </header>
      <form className="workflow-form" onSubmit={handleSubmit}>
        <label htmlFor="outfit-character-id">Character</label>
        <select autoFocus id="outfit-character-id" value={characterEntityId} onChange={(event) => setCharacterEntityId(event.target.value)} required>
          {characters.length === 0 ? <option value="">No Character Canon entities available</option> : characters.map((character) => <option key={character.id} value={character.id}>{character.name}</option>)}
        </select>
        <label htmlFor="outfit-wardrobe">Wardrobe description</label>
        <textarea id="outfit-wardrobe" value={wardrobeDescription} onChange={(event) => setWardrobeDescription(event.target.value)} required rows={4} placeholder="charcoal long-sleeve top, dark utility trousers, black boots, black watch on left wrist" />
        <ProviderModelFields projectRootPath={projectRootPath} value={providerSelection} mediaType="image" requiresReferences onChange={setProviderSelection} />
        <div className="workflow-form-actions">
          <button type="submit" disabled={pending}>Create workflow run</button>
          <button type="button" onClick={onCancel} disabled={pending}>Cancel</button>
        </div>
      </form>
    </section>
  );
}
