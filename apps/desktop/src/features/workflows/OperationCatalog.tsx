import type { SkillOperation } from "@cinematic/domain";
import { operationLabel } from "./labels";

interface OperationCatalogProps {
  operations: SkillOperation[];
  onSelect: (operation: SkillOperation, trigger: HTMLButtonElement) => void;
}

const TOOL_HINTS: Record<string, string> = {
  "character.create_face_lock": "The approved face every scene uses to keep this character consistent.",
  "character.create_outfit": "What this character wears — generated on top of the approved face.",
  "character.create_character_sheet": "A full-body reference sheet from the approved outfit.",
};

const TOOL_ACTIONS: Record<string, string> = {
  "character.create_face_lock": "Generate face reference",
  "character.create_outfit": "Generate outfit",
  "character.create_character_sheet": "Generate character sheet",
};

/**
 * "Generation tools" section: only operations that have an entry form in this
 * workspace. Content-generation operations that run from their context (world
 * plates, keyframes, QA) are intentionally absent — they start where the
 * content lives.
 */
export function OperationCatalog({ operations, onSelect }: OperationCatalogProps) {
  return (
    <section className="workflow-catalog" aria-labelledby="workflow-catalog-title">
      <header className="workflow-panel-header">
        <div>
          <h2 id="workflow-catalog-title">Generation tools</h2>
          <p>Generate character references here. World backdrops, shot keyframes, and videos start from their own screens.</p>
        </div>
      </header>
      {operations.length === 0 ? (
        <p role="status">No tools available.</p>
      ) : (
        <ul className="workflow-operation-list">
          {operations.map((operation) => (
            <li key={operation.id}>
              <div>
                <h3>{operationLabel(operation.id)}</h3>
                <p>{TOOL_HINTS[operation.id] ?? operation.description}</p>
              </div>
              <button type="button" onClick={(event) => onSelect(operation, event.currentTarget)}>
                {TOOL_ACTIONS[operation.id] ?? `Generate ${operationLabel(operation.id).toLowerCase()}`}
              </button>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
