import { useEffect, useRef, useState } from "react";
import type { SkillOperation, WorkflowCharacterOption, WorkflowRunDetail, WorkflowRunRecord } from "@cinematic/domain";
import { describeError } from "../../lib/errors";
import { advanceWorkflowRun, createWorkflowRun, getWorkflowRun, listSkillOperations, listWorkflowCharacters, listWorkflowRuns } from "./api";
import { CreateFaceLockForm } from "./CreateFaceLockForm";
import { CreateOutfitForm } from "./CreateOutfitForm";
import { CreateCharacterSheetForm } from "./CreateCharacterSheetForm";
import { OperationCatalog } from "./OperationCatalog";
import { WorkflowRunView } from "./WorkflowRunView";
import { formatRunTime, runTitle, runStatusLabel } from "./labels";

/** Maps an operation id to the skill that owns it (registry key + version). */
function resolveSkillRef(operationId: string): [string, string] {
  if (operationId.startsWith("character.")) return ["character-builder", "1.1.0"];
  if (operationId.startsWith("world.")) return ["world-builder", "1.0.0"];
  if (operationId.startsWith("scene.")) return ["scene-builder", "1.0.0"];
  if (operationId.startsWith("asset.")) return ["visual-qa", "1.0.0"];
  throw new Error(`No skill is registered for operation ${operationId}`);
}

/** Character operations have entry forms here; everything else runs from the
 * screen where its content lives (Assets, Worlds, Scenes). */
const CHARACTER_OPERATION_IDS = [
  "character.create_face_lock",
  "character.create_outfit",
  "character.create_character_sheet",
];

const ACTIVE_RUN_STATUSES = new Set(["created", "running", "waiting_for_approval", "ready_for_execution"]);

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

  const activeRuns = runs.filter((run) => ACTIVE_RUN_STATUSES.has(run.status));
  const pastRuns = runs.filter((run) => !ACTIVE_RUN_STATUSES.has(run.status));
  const runnableTools = operations.filter((operation) => CHARACTER_OPERATION_IDS.includes(operation.id));

  return (
    <div className="workflow-workspace">
      {error ? <p role="alert">{error}</p> : null}
      <div className="workflow-recent" aria-labelledby="recent-runs-title" aria-busy={loading}>
        <header className="workflow-panel-header">
          <div>
            <h2 id="recent-runs-title">Generations</h2>
            <p>Every image and video the AI made for this project, kept even after you close the app.</p>
          </div>
        </header>
        {loading ? <p className="workflow-loading" role="status">Loading generation history…</p> : runs.length === 0 ? (
          <div className="empty-state" role="status">
            <p>Nothing generated yet</p>
            <p>Results appear here the moment you generate a face, outfit, world backdrop, keyframe, or video — start from the screen where you're working, or use the tools below.</p>
          </div>
        ) : (
          <>
            {activeRuns.length ? (
              <section aria-label="Generations in progress">
                <h3>In progress</h3>
                <ul>{activeRuns.map((run) => <li key={run.id}><button type="button" onClick={(event) => openRun(run.id, event.currentTarget)} disabled={pending}><span>{runTitle(run)}</span><span className={`workflow-status workflow-status--${run.status}`}><span aria-hidden="true">●</span><span>{runStatusLabel(run.status)}</span></span></button></li>)}</ul>
              </section>
            ) : null}
            {pastRuns.length ? (
              <section aria-label="Completed generations">
                <h3>History</h3>
                <ul>{pastRuns.map((run) => <li key={run.id}><button type="button" onClick={(event) => openRun(run.id, event.currentTarget)} disabled={pending}><span>{runTitle(run)}</span><span className={`workflow-status workflow-status--${run.status}`}><span aria-hidden="true">●</span><span>{runStatusLabel(run.status)}</span></span><time>{formatRunTime(run.createdAt)}</time></button></li>)}</ul>
              </section>
            ) : null}
          </>
        )}
      </div>
      <OperationCatalog operations={runnableTools} onSelect={selectOperation} />
      {selectedOperation ? (
        isCharacterOperation(selectedOperation.id) ? (
          selectedOperation.id === "character.create_outfit" ? (
            <CreateOutfitForm projectRootPath={projectRootPath} characters={characters} pending={pending} onCancel={cancelOperation} onSubmit={handleCreate} />
          ) : selectedOperation.id === "character.create_character_sheet" ? (
            <CreateCharacterSheetForm projectRootPath={projectRootPath} characters={characters} pending={pending} onCancel={cancelOperation} onSubmit={handleCreate} />
          ) : selectedOperation.id === "character.create_face_lock" ? (
            <CreateFaceLockForm projectRootPath={projectRootPath} characters={characters} pending={pending} onCancel={cancelOperation} onSubmit={handleCreate} />
          ) : null
        ) : null
      ) : null}
      {selectedRun ? <WorkflowRunView projectRootPath={projectRootPath} detail={selectedRun} onChange={handleRunChange} /> : null}
    </div>
  );
}
