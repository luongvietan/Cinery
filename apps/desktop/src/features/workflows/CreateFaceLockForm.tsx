import { type FormEvent, useState } from "react";
import type { WorkflowCharacterOption } from "@cinematic/domain";

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
  const [visualSpec, setVisualSpec] = useState<Record<(typeof VISUAL_FIELDS)[number], string>>(
    Object.fromEntries(VISUAL_FIELDS.map((field) => [field, ""])) as Record<(typeof VISUAL_FIELDS)[number], string>,
  );

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    await onSubmit({ projectRootPath, characterEntityId, visualSpec, baselineWardrobe });
  }

  return (
    <section className="workflow-editor" aria-labelledby="face-lock-form-title">
      <header className="workflow-panel-header">
        <div>
          <h2 id="face-lock-form-title">Create Face Lock</h2>
          <p>Draft Canon is excluded. Locked Canon is captured when the run advances.</p>
        </div>
      </header>
      <form className="workflow-form" onSubmit={handleSubmit}>
        <label htmlFor="workflow-character-id">Character</label>
        <select autoFocus id="workflow-character-id" value={characterEntityId} onChange={(event) => setCharacterEntityId(event.target.value)} required aria-describedby="workflow-character-help">
          {characters.length === 0 ? <option value="">No Character Canon entities available</option> : characters.map((character) => <option key={character.id} value={character.id}>{character.name}</option>)}
        </select>
        <span id="workflow-character-help" className="workflow-field-help">Only Character entities from this project are available.</span>
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
        <div className="workflow-form-actions">
          <button type="submit" disabled={pending}>Create workflow run</button>
          <button type="button" onClick={onCancel} disabled={pending}>Cancel</button>
        </div>
      </form>
    </section>
  );
}
