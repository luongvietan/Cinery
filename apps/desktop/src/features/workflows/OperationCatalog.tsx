import type { SkillOperation } from "@cinematic/domain";

interface OperationCatalogProps {
  operations: SkillOperation[];
  onSelect: (operation: SkillOperation, trigger: HTMLButtonElement) => void;
}

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
              <span className="workflow-operation-family">Character Builder</span>
              <h3>{operation.name}</h3>
              <p>{operation.description}</p>
            </div>
            <button type="button" onClick={(event) => onSelect(operation, event.currentTarget)}>
              Create Face Lock
            </button>
          </li>
        ))}
      </ul>
    </section>
  );
}
