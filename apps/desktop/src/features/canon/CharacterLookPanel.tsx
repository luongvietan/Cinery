import { useEffect, useState } from "react";
import type { AssetSummary, SkillOperation, WorkflowCharacterOption, WorkflowRunDetail } from "@cinematic/domain";
import { deriveGenerationResultContext } from "@cinematic/domain";
import { describeError } from "../../lib/errors";
import { convertFileSrc } from "@tauri-apps/api/core";
import { joinProjectRelativePath } from "../assets/paths";
import { getAssetWithVersions } from "../assets/api";
import { listAssets } from "../assets/api";
import { advanceWorkflowRun, createWorkflowRun, listSkillOperations } from "../workflows/api";
import { GenerationResults } from "../generation/GenerationResults";
import { listGenerationResults } from "../generation/api";
import { WorkflowRunView } from "../workflows/WorkflowRunView";
import { CreateFaceLockForm } from "../workflows/CreateFaceLockForm";
import { CreateOutfitForm } from "../workflows/CreateOutfitForm";
import { CreateCharacterSheetForm } from "../workflows/CreateCharacterSheetForm";
import { ThinkingIndicator } from "../../components/ThinkingIndicator";

/**
 * Look-step model: face → outfit → sheet. The backend enforces the same
 * prerequisites (no outfit without an approved face, no sheet without an
 * approved outfit); this panel makes them visible *before* a run fails.
 */
type LookStepKey = "face" | "outfit" | "sheet";

interface LookStep {
  key: LookStepKey;
  title: string;
  assetType: string;
  operationId: string;
  doneLabel: string;
  requirement: string;
}

const LOOK_STEPS: LookStep[] = [
  { key: "face", title: "Face reference", assetType: "face_lock", operationId: "character.create_face_lock", doneLabel: "Face approved", requirement: "Start here — every later reference uses this face." },
  { key: "outfit", title: "Outfit", assetType: "outfit", operationId: "character.create_outfit", doneLabel: "Outfit approved", requirement: "Needs an approved face first." },
  { key: "sheet", title: "Character sheet", assetType: "character_sheet", operationId: "character.create_character_sheet", doneLabel: "Sheet approved", requirement: "Needs an approved outfit first." },
];

function resolveSkillRef(operationId: string): [string, string] {
  if (operationId.startsWith("character.")) return ["character-builder", "1.1.0"];
  throw new Error(`No skill is registered for operation ${operationId}`);
}

interface LookRunState {
  step: LookStep;
  detail: WorkflowRunDetail;
  context: ReturnType<typeof deriveGenerationResultContext>;
}

/**
 * Character look references: generate, review, and approve the character's
 * face, outfit, and sheet without leaving the character. Lives inside the
 * Story workspace next to the character's story sections.
 */
export function CharacterLookPanel({ projectRootPath, character }: { projectRootPath: string; character: WorkflowCharacterOption }) {
  const [assets, setAssets] = useState<AssetSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [pendingStep, setPendingStep] = useState<LookStep | null>(null);
  const [formStep, setFormStep] = useState<LookStep | null>(null);
  const [run, setRun] = useState<LookRunState | null>(null);
  const [previews, setPreviews] = useState<Partial<Record<LookStepKey, string>>>({});

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    setRun(null);
    setPreviews({});
    listAssets(projectRootPath)
      .then((next) => { if (!cancelled) setAssets(next); })
      .catch((caught) => { if (!cancelled) setError(describeError(caught)); })
      .finally(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [projectRootPath, character.id]);

  // Approved look asset per step (owned by this character).
  const assetFor = (step: LookStep): AssetSummary | null =>
    assets.find((asset) => asset.type === step.assetType && asset.ownerEntityId === character.id && asset.canonicalVersionId !== null) ?? null;
  const faceApproved = assetFor(LOOK_STEPS[0]) !== null;
  const outfitApproved = assetFor(LOOK_STEPS[1]) !== null;

  // Load the approved image preview for each step.
  useEffect(() => {
    if (loading) return;
    let cancelled = false;
    for (const step of LOOK_STEPS) {
      const asset = assetFor(step);
      if (!asset?.previewThumbnailPath) continue;
      getAssetWithVersions(projectRootPath, asset.id)
        .then((detail) => {
          if (cancelled) return;
          const canonical = detail.versions.find((version) => version.id === detail.asset.canonicalVersionId);
          const path = canonical?.filePath ?? canonical?.thumbnailPath;
          if (path) {
            const url = convertFileSrc(joinProjectRelativePath(projectRootPath, path));
            setPreviews((current) => ({ ...current, [step.key]: url }));
          }
        })
        .catch(() => undefined);
    }
    return () => { cancelled = true; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [loading, assets, projectRootPath, character.id]);

  async function openForm(step: LookStep) {
    setRun(null);
    setFormStep(step);
  }

  async function handleCreate(input: Record<string, unknown>) {
    if (!formStep) return;
    setPendingStep(formStep);
    setError(null);
    try {
      const created = await createWorkflowRun(
        projectRootPath,
        ...resolveSkillRef(formStep.operationId),
        formStep.operationId,
        { ...input, characterEntityId: character.id },
      );
      const waiting = await advanceWorkflowRun(projectRootPath, created.run.id);
      setFormStep(null);
      await applyRun(formStep, waiting);
    } catch (caught) {
      setError(describeError(caught));
    } finally {
      setPendingStep(null);
    }
  }

  /** Attach fetched candidates when a run completes so review is visual. */
  async function applyRun(step: LookStep, detail: WorkflowRunDetail) {
    let context: LookRunState["context"] = null;
    let assetList = assets;
    if (detail.run.status === "completed") {
      try {
        const [operations, assetSummaries, resultSets] = await Promise.all([
          listSkillOperations(),
          listAssets(projectRootPath),
          listGenerationResults(projectRootPath, detail.run.id),
        ]);
        const operation = operations.find((candidate) => candidate.id === detail.run.operationId) ?? null;
        if (operation) {
          const derived = deriveGenerationResultContext(detail, operation);
          if (derived && resultSets.some((resultSet) => resultSet.artifacts.length > 0)) {
            context = { ...derived, resultSets };
          }
        }
        assetList = assetSummaries;
      } catch {
        // Run view still shows the completed run without candidates.
      }
    }
    setRun({ step, detail, context });
    setAssets(assetList);
  }

  function handleRunChange(next: WorkflowRunDetail) {
    if (!run) return;
    if (next.run.status === "completed") {
      void applyRun(run.step, next);
      return;
    }
    setRun({ ...run, detail: next });
  }

  async function handlePromoted() {
    setRun(null);
    try {
      setAssets(await listAssets(projectRootPath));
    } catch (caught) {
      setError(describeError(caught));
    }
  }

  const characters = [character];

  return (
    <section className="look-panel" aria-labelledby="look-panel-title">
      <header className="canon-panel-header">
        <div>
          <h3 id="look-panel-title">Look references</h3>
          <p>The approved face, outfit, and sheet every scene uses to keep {character.name} consistent. Generate and pick them here.</p>
        </div>
      </header>
      {error ? <p role="alert">{error}</p> : null}
      {loading ? (
        <p className="workspace-loading" role="status"><ThinkingIndicator state="working" /> Loading look references…</p>
      ) : (
        <div className="look-steps">
          {LOOK_STEPS.map((step) => {
            const asset = assetFor(step);
            const state: "approved" | "missing" | "blocked" = asset ? "approved" : (step.key === "outfit" && !faceApproved) || (step.key === "sheet" && !outfitApproved) ? "blocked" : "missing";
            const stepPreview = previews[step.key] ?? null;
            return (
              <article key={step.key} className={`look-step look-step--${state}`} data-state={state}>
                <header className="look-step__header">
                  <h4>{step.title}</h4>
                  {state === "approved" ? (
                    <span className="asset-version-badge asset-version-badge--canonical">Approved</span>
                  ) : state === "blocked" ? (
                    <span className="look-step__state" role="status">{step.requirement}</span>
                  ) : null}
                </header>
                {stepPreview ? (
                  <img className="look-step__preview" src={stepPreview} alt={`${character.name} approved ${step.title}`} loading="lazy" />
                ) : asset ? (
                  <span className="look-step__preview look-step__preview--empty" aria-hidden="true" />
                ) : null}
                <div className="look-step__actions">
                  {state === "approved" ? (
                    <button type="button" className="asset-secondary-button" onClick={() => void openForm(step)}>Generate another</button>
                  ) : state === "blocked" ? (
                    <button type="button" className="asset-secondary-button" disabled aria-describedby={`look-step-help-${step.key}`}>{step.requirement}</button>
                  ) : (
                    <button type="button" className="look-step__action" onClick={() => void openForm(step)}>Generate {step.title.toLowerCase()}</button>
                  )}
                </div>
                {state !== "approved" ? <span id={`look-step-help-${step.key}`} className="look-step__help">{step.requirement}</span> : null}
              </article>
            );
          })}
        </div>
      )}
      {formStep ? (
        formStep.key === "face" ? (
          <CreateFaceLockForm projectRootPath={projectRootPath} characters={characters} pending={pendingStep === formStep} onCancel={() => setFormStep(null)} onSubmit={handleCreate} />
        ) : formStep.key === "outfit" ? (
          <CreateOutfitForm projectRootPath={projectRootPath} characters={characters} pending={pendingStep === formStep} onCancel={() => setFormStep(null)} onSubmit={handleCreate} />
        ) : (
          <CreateCharacterSheetForm projectRootPath={projectRootPath} characters={characters} pending={pendingStep === formStep} onCancel={() => setFormStep(null)} onSubmit={handleCreate} />
        )
      ) : null}
      {run ? (
        run.detail.run.status === "completed" && run.context ? (
          <GenerationResults
            projectRootPath={projectRootPath}
            context={run.context}
            assets={assets}
            onPromoted={() => void handlePromoted()}
          />
        ) : (
          <WorkflowRunView projectRootPath={projectRootPath} detail={run.detail} onChange={handleRunChange} />
        )
      ) : null}
    </section>
  );
}
