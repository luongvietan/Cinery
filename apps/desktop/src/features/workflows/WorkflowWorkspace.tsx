import { useEffect, useRef, useState } from "react";
import type { SkillOperation, WorkflowCharacterOption, WorkflowRunDetail, WorkflowRunRecord } from "@cinematic/domain";
import { describeError } from "../../lib/errors";
import { advanceWorkflowRun, createWorkflowRun, getWorkflowRun, listSkillOperations, listWorkflowCharacters, listWorkflowRuns } from "./api";
import { CreateFaceLockForm } from "./CreateFaceLockForm";
import { CreateOutfitForm } from "./CreateOutfitForm";
import { CreateCharacterSheetForm } from "./CreateCharacterSheetForm";
import { OperationCatalog } from "./OperationCatalog";
import { WorkflowRunView } from "./WorkflowRunView";
import { humanizeWorkflowStatus } from "./format";

/** Maps an operation id to the skill that owns it (registry key + version). */
function resolveSkillRef(operationId: string): [string, string] {
  if (operationId.startsWith("character.")) return ["character-builder", "1.1.0"];
  if (operationId.startsWith("world.")) return ["world-builder", "1.0.0"];
  if (operationId.startsWith("scene.")) return ["scene-builder", "1.0.0"];
  if (operationId.startsWith("asset.")) return ["visual-qa", "1.0.0"];
  throw new Error(`No skill is registered for operation ${operationId}`);
}

/**
 * Operations whose input forms are not offered in this workspace. They have
 * real context entry points elsewhere (Asset Inspector, Worlds, Scenes) and
 * must never fall through to a character form.
 */
const OPERATIONS_WITH_EXTERNAL_ENTRY = ["asset.", "world.", "scene."];

function operationEntryHint(operationId: string): string {
  if (operationId.startsWith("asset.")) {
    return "Run Visual QA and Repair from an asset version in the Assets panel.";
  }
  if (operationId.startsWith("world.")) {
    return "Generate a world plate from the World detail in the Worlds panel.";
  }
  if (operationId.startsWith("scene.")) {
    return "Generate a shot keyframe from its shot in the Scenes panel.";
  }
  return "This operation is started from its production context.";
}

function isCharacterOperation(operationId: string): boolean {
  return operationId.startsWith("character.");
}

export function WorkflowWorkspace({ projectRootPath }: { projectRootPath: string }) {
  const [operations, setOperations] = useState<SkillOperation[]>([]);
  const [characters, setCharacters] = useState<WorkflowCharacterOption[]>([]);
  const [runs, setRuns] = useState<WorkflowRunRecord[]>([]);
  const [selectedOperation, setSelectedOperation] = useState<SkillOperation | null>(null);
  const [selectedRun, setSelectedRun] = useState<WorkflowRunDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const returnFocusRef = useRef<HTMLButtonElement | null>(null);

  useEffect(() => {
    let cancelled = false;
    Promise.all([listSkillOperations(), listWorkflowCharacters(projectRootPath), listWorkflowRuns(projectRootPath)])
      .then(([nextOperations, nextCharacters, nextRuns]) => {
        if (!cancelled) { setOperations(nextOperations); setCharacters(nextCharacters); setRuns(nextRuns); }
      })
      .catch((reason) => { if (!cancelled) setError(describeError(reason)); })
      .finally(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [projectRootPath]);

  function selectOperation(operation: SkillOperation, trigger: HTMLButtonElement) {
    returnFocusRef.current = trigger;
    setSelectedRun(null);
    setSelectedOperation(operation);
  }

  function cancelOperation() {
    setSelectedOperation(null);
    requestAnimationFrame(() => returnFocusRef.current?.focus());
  }

  async function handleCreate(input: Record<string, unknown>) {
    if (!selectedOperation) return;
    setPending(true); setError(null);
    try {
      const created = await createWorkflowRun(
        projectRootPath,
        ...resolveSkillRef(selectedOperation.id),
        selectedOperation.id,
        input,
      );
      const waiting = await advanceWorkflowRun(projectRootPath, created.run.id);
      setSelectedRun(waiting); setSelectedOperation(null);
      setRuns(await listWorkflowRuns(projectRootPath));
    } catch (reason) { setError(describeError(reason)); }
    finally { setPending(false); }
  }

  async function openRun(runId: string, trigger: HTMLButtonElement) {
    returnFocusRef.current = trigger;
    setSelectedOperation(null);
    setPending(true); setError(null);
    try { setSelectedRun(await getWorkflowRun(projectRootPath, runId)); }
    catch (reason) { setError(describeError(reason)); }
    finally { setPending(false); }
  }

  function handleRunChange(nextDetail: WorkflowRunDetail) {
    setSelectedRun(nextDetail);
    setRuns((current) => current.map((run) => run.id === nextDetail.run.id ? nextDetail.run : run));
  }

  return (
    <div className="workflow-workspace">
      {error ? <p role="alert">{error}</p> : null}
      <div className="workflow-overview" aria-busy={loading}>
        <OperationCatalog operations={operations} onSelect={selectOperation} />
        <section className="workflow-recent" aria-labelledby="recent-runs-title" aria-busy={loading}>
          <header className="workflow-panel-header"><div><h2 id="recent-runs-title">Recent runs</h2><p>Persisted workflow state. Opening a run never advances it.</p></div></header>
          {loading ? <p className="workflow-loading" role="status">Loading workflow history…</p> : runs.length === 0 ? <p className="workflow-loading">No workflow runs yet.</p> : (
            <ul>{runs.map((run) => <li key={run.id}><button type="button" onClick={(event) => openRun(run.id, event.currentTarget)} disabled={pending}><span>{run.operationId}</span><span className={`workflow-status workflow-status--${run.status}`}><span aria-hidden="true">●</span><span>{humanizeWorkflowStatus(run.status)}</span></span><span>{run.skillId}@{run.skillVersion}</span></button></li>)}</ul>
          )}
        </section>
      </div>
      {selectedOperation ? (
        isCharacterOperation(selectedOperation.id) ? (
          selectedOperation.id === "character.create_outfit" ? (
            <CreateOutfitForm projectRootPath={projectRootPath} characters={characters} pending={pending} onCancel={cancelOperation} onSubmit={handleCreate} />
          ) : selectedOperation.id === "character.create_character_sheet" ? (
            <CreateCharacterSheetForm projectRootPath={projectRootPath} characters={characters} pending={pending} onCancel={cancelOperation} onSubmit={handleCreate} />
          ) : selectedOperation.id === "character.create_face_lock" ? (
            <CreateFaceLockForm projectRootPath={projectRootPath} characters={characters} pending={pending} onCancel={cancelOperation} onSubmit={handleCreate} />
          ) : (
            <section aria-label="Unsupported operation" className="workflow-panel-header">
              <p role="status">
                {`"${selectedOperation.id}" has no form in this workspace. ${operationEntryHint(selectedOperation.id)}`}
              </p>
            </section>
          )
        ) : (
          <section aria-label="Unsupported operation" className="workflow-panel-header">
            <p role="status">
              {`"${selectedOperation.id}" is started from its production context. ${operationEntryHint(selectedOperation.id)}`}
            </p>
          </section>
        )
      ) : null}
      {selectedRun ? <WorkflowRunView projectRootPath={projectRootPath} detail={selectedRun} onChange={handleRunChange} /> : null}
    </div>
  );
}
