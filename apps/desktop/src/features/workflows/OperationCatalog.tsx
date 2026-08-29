import type { SkillOperation } from "@cinematic/domain";

interface OperationCatalogProps {
  operations: SkillOperation[];
  onSelect: (operation: SkillOperation, trigger: HTMLButtonElement) => void;
}

const OPERATION_LABELS: Record<string, string> = {
  "character.create_face_lock": "Create Face Lock",
  "character.create_outfit": "Create Outfit",
  "character.create_character_sheet": "Create Character Sheet",
  "asset.run_visual_qa": "Run Visual QA",
  "asset.repair_failed_qa": "Repair Failed Visual QA",
};

export function OperationCatalog({ operations, onSelect }: OperationCatalogProps) {
  return (
    <section className="workflow-catalog" aria-labelledby="workflow-catalog-title">
      <header className="workflow-panel-header">
        <div>
          <h2 id="workflow-catalog-title">Available operations</h2>
          <p>Versioned production procedures registered in this application.</p>
        </div>
      </header>
      <ul className="workflow-operation-list">
        {operations.map((operation) => (
          <li key={operation.id}>
            <div>
              <span className="workflow-operation-family">{operation.id.split(".")[0]}</span>
              <h3>{operation.name}</h3>
              <p>{operation.description}</p>
            </div>
            <button type="button" onClick={(event) => onSelect(operation, event.currentTarget)}>
              {OPERATION_LABELS[operation.id] ?? operation.name}
            </button>
          </li>
        ))}
      </ul>
    </section>
  );
}
