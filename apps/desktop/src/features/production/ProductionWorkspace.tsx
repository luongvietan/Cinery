import { useEffect, useState } from "react";
import type { AssetSummary, AssetVersion, SkillOperation, WorkflowCharacterOption, WorkflowRunDetail, WorkflowRunRecord } from "@cinematic/domain";
import { describeError } from "../../lib/errors";
import { listAssets } from "../assets/api";
import { deriveGenerationResultContext } from "@cinematic/domain";
import { advanceWorkflowRun, createWorkflowRun, getWorkflowRun, listSkillOperations, listWorkflowCharacters, listWorkflowRuns } from "../workflows/api";
import { WorkflowRunView } from "../workflows/WorkflowRunView";
import { GenerationResults } from "./GenerationResults";
import { listGenerationResults } from "./api";
import { CharacterBuilderOperation } from "./CharacterBuilderOperation";
import { AiDirectorBar } from "./AiDirectorBar";
import { ThinkingIndicator } from "../../components/ThinkingIndicator";

export function ProductionWorkspace({ projectRootPath }: { projectRootPath: string }) {
  const [operations, setOperations] = useState<SkillOperation[]>([]);
  const [characters, setCharacters] = useState<WorkflowCharacterOption[]>([]);
  const [assets, setAssets] = useState<AssetSummary[]>([]);
  const [runs, setRuns] = useState<WorkflowRunRecord[]>([]);
  const [selectedRun, setSelectedRun] = useState<WorkflowRunDetail | null>(null);
  const [showCreate, setShowCreate] = useState(false);
  const [results, setResults] = useState<Awaited<ReturnType<typeof listGenerationResults>>>([]);
  const [pending, setPending] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const faceAssets = assets.filter((asset) => asset.type === "face_lock" && asset.canonicalVersionId);
  const targetAsset = faceAssets[0] ?? null;

  async function load() {
    setPending(true); setError(null);
    try { const [nextOperations, nextCharacters, nextAssets, nextRuns] = await Promise.all([listSkillOperations(), listWorkflowCharacters(projectRootPath), listAssets(projectRootPath), listWorkflowRuns(projectRootPath)]); setOperations(nextOperations); setCharacters(nextCharacters); setAssets(nextAssets); setRuns(nextRuns); }
    catch (reason) { setError(describeError(reason)); }
    finally { setPending(false); }
  }
  useEffect(() => { void load(); }, [projectRootPath]);

  async function refreshResults(runId: string) { try { setResults(await listGenerationResults(projectRootPath, runId)); } catch (reason) { setError(describeError(reason)); } }
  async function handleCreate(input: Record<string, unknown>) { setPending(true); setError(null); try { const created = await createWorkflowRun(projectRootPath, "character-builder", "1.1.0", "character.create_face_lock", input); const waiting = await advanceWorkflowRun(projectRootPath, created.run.id); setSelectedRun(waiting); setShowCreate(false); setRuns(await listWorkflowRuns(projectRootPath)); } catch (reason) { setError(describeError(reason)); } finally { setPending(false); } }
  async function openRun(run: WorkflowRunRecord) { setPending(true); try { const detail = await getWorkflowRun(projectRootPath, run.id); setSelectedRun(detail); if (detail.run.status === "completed") await refreshResults(run.id); } catch (reason) { setError(describeError(reason)); } finally { setPending(false); } }
  function handleRunChange(detail: WorkflowRunDetail) { setSelectedRun(detail); setRuns((current) => current.map((run) => run.id === detail.run.id ? detail.run : run)); if (detail.run.status === "completed") void refreshResults(detail.run.id); }
  function handlePromoted(_targetAssetId: string, _versionId: string) { void load(); }

  const selectedOperation = selectedRun
    ? operations.find((operation) => operation.id === selectedRun.run.operationId) ?? null
    : null;
  const resultContext = selectedRun && selectedOperation
    ? deriveGenerationResultContext(selectedRun, selectedOperation)
    : null;
  const contextWithResults = resultContext
    ? { ...resultContext, resultSets: results }
    : null;

  return <div className="production-workspace">{error ? <p role="alert">{error}</p> : null}<section className="production-hero"><div><span className="production-kicker">Production / Character Builder</span><h2>Production</h2><p>Generate durable candidates from locked Canon and save only the result you explicitly promote.</p></div><span className="production-hero-mark" aria-hidden="true">P5</span></section><AiDirectorBar projectRootPath={projectRootPath} />{pending && !operations.length ? <p role="status" className="workspace-loading"><ThinkingIndicator state="working" /> Loading production…</p> : null}{!showCreate && !selectedRun ? <section className="production-landing"><article className="production-operation-card"><div><span className="production-kicker">Character Builder</span><h3>{operations.find((operation) => operation.id === "character.create_face_lock")?.name ?? "Create Face Lock"}</h3><p>{operations.find((operation) => operation.id === "character.create_face_lock")?.description ?? "Create a consistent production reference for a character."}</p></div><button type="button" onClick={() => setShowCreate(true)}>Create Face Lock</button>{!targetAsset ? <small>No canonical face yet — the first Face Lock can be generated without one.</small> : null}</article><section className="production-recent"><header><div><span className="production-kicker">History</span><h3>Recent production</h3></div></header>{runs.length ? <ul>{runs.slice(0, 5).map((run) => <li key={run.id}><button type="button" onClick={() => void openRun(run)}><span>{run.operationId}</span><span>{run.status}</span></button></li>)}</ul> : <p>No production runs yet.</p>}</section></section> : null}{showCreate ? <CharacterBuilderOperation projectRootPath={projectRootPath} characters={characters} sourceAssets={faceAssets} pending={pending} onCancel={() => setShowCreate(false)} onSubmit={handleCreate} /> : null}{selectedRun ? <WorkflowRunView projectRootPath={projectRootPath} detail={selectedRun} onChange={handleRunChange} /> : null}{selectedRun?.run.status === "completed" && contextWithResults ? <GenerationResults projectRootPath={projectRootPath} context={contextWithResults} assets={assets} onPromoted={handlePromoted} /> : null}</div>;
}
