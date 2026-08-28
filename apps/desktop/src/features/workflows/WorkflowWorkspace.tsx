import { useEffect, useState } from "react";
import type { SkillOperation, WorkflowCharacterOption, WorkflowRunDetail, WorkflowRunRecord } from "@cinematic/domain";
import { describeError } from "../../lib/errors";
import { advanceWorkflowRun, createWorkflowRun, getWorkflowRun, listSkillOperations, listWorkflowCharacters, listWorkflowRuns } from "./api";
import { CreateFaceLockForm } from "./CreateFaceLockForm";
import { OperationCatalog } from "./OperationCatalog";
import { WorkflowRunView } from "./WorkflowRunView";
import { humanizeWorkflowStatus } from "./format";

export function WorkflowWorkspace({ projectRootPath }: { projectRootPath: string }) {
  const [operations, setOperations] = useState<SkillOperation[]>([]);
  const [characters, setCharacters] = useState<WorkflowCharacterOption[]>([]);
  const [runs, setRuns] = useState<WorkflowRunRecord[]>([]);
  const [selectedOperation, setSelectedOperation] = useState<SkillOperation | null>(null);
  const [selectedRun, setSelectedRun] = useState<WorkflowRunDetail | null>(null);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    Promise.all([listSkillOperations(), listWorkflowCharacters(projectRootPath), listWorkflowRuns(projectRootPath)])
      .then(([nextOperations, nextCharacters, nextRuns]) => {
        if (!cancelled) { setOperations(nextOperations); setCharacters(nextCharacters); setRuns(nextRuns); }
      })
      .catch((reason) => { if (!cancelled) setError(describeError(reason)); });
    return () => { cancelled = true; };
  }, [projectRootPath]);

  async function handleCreate(input: Record<string, unknown>) {
    setPending(true); setError(null);
    try {
      const created = await createWorkflowRun(projectRootPath, input);
      const waiting = await advanceWorkflowRun(projectRootPath, created.run.id);
      setSelectedRun(waiting); setSelectedOperation(null);
      setRuns(await listWorkflowRuns(projectRootPath));
    } catch (reason) { setError(describeError(reason)); }
    finally { setPending(false); }
  }

  async function openRun(runId: string) {
    setPending(true); setError(null);
    try { setSelectedRun(await getWorkflowRun(projectRootPath, runId)); }
    catch (reason) { setError(describeError(reason)); }
    finally { setPending(false); }
  }

  return (
    <div className="workflow-workspace">
      {error ? <p role="alert">{error}</p> : null}
      <div className="workflow-overview">
        <OperationCatalog operations={operations} onSelect={setSelectedOperation} />
        <section className="workflow-recent" aria-labelledby="recent-runs-title">
          <header className="workflow-panel-header"><div><h2 id="recent-runs-title">Recent runs</h2><p>Persisted workflow state. Opening a run never advances it.</p></div></header>
          {runs.length === 0 ? <p>No workflow runs yet.</p> : (
            <ul>{runs.map((run) => <li key={run.id}><button type="button" onClick={() => openRun(run.id)} disabled={pending}><span>{run.operationId}</span><span className={`workflow-status workflow-status--${run.status}`}><span aria-hidden="true">●</span><span>{humanizeWorkflowStatus(run.status)}</span></span><span>{run.skillId}@{run.skillVersion}</span></button></li>)}</ul>
          )}
        </section>
      </div>
      {selectedOperation ? <CreateFaceLockForm projectRootPath={projectRootPath} characters={characters} pending={pending} onCancel={() => setSelectedOperation(null)} onSubmit={handleCreate} /> : null}
      {selectedRun ? <WorkflowRunView projectRootPath={projectRootPath} detail={selectedRun} onChange={setSelectedRun} /> : null}
    </div>
  );
}
