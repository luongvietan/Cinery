import { useMemo, useState } from "react";
import type { AssetSummary, AssetVersion, GenerationResultContext } from "@cinematic/domain";
import { ActionButton } from "../../components/ActionButton";
import { describeError } from "../../lib/errors";
import { createAsset } from "../assets/api";
import { GenerationResultCard } from "./GenerationResultCard";
import { PromoteArtifactDialog } from "./PromoteArtifactDialog";
import { promoteGeneratedArtifact } from "./api";

const ASSET_TYPE_LABELS: Record<string, string> = {
  face_lock: "Face",
  outfit: "Outfit",
  character_sheet: "Character Sheet",
  world_plate: "World Plate",
  shot_keyframe: "Shot Keyframe",
  prop_plate: "Prop Plate",
  image: "Image",
  video: "Video",
  audio: "Audio",
};

interface GenerationResultsProps {
  projectRootPath: string;
  context: GenerationResultContext;
  assets: AssetSummary[];
  onPromoted?(targetAssetId: string, versionId: string): void;
}

export function GenerationResults({ projectRootPath, context, assets, onPromoted }: GenerationResultsProps) {
  const resultSets = context.resultSets;
  const artifacts = useMemo(() => resultSets.flatMap((result) => result.artifacts), [resultSets]);
  const [selectedId, setSelectedId] = useState(artifacts[0]?.artifact.id ?? "");
  const [promoting, setPromoting] = useState(false);
  const [selectedTargetId, setSelectedTargetId] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [createPending, setCreatePending] = useState(false);
  const [createName, setCreateName] = useState("");
  const [createError, setCreateError] = useState<string | null>(null);
  const [extraAsset, setExtraAsset] = useState<AssetSummary | null>(null);
  if (!artifacts.length) return null;

  const operationLabel =
    ASSET_TYPE_LABELS[context.expectedAssetType] ?? context.expectedAssetType;
  const eligibleAssets = assets.filter(
    (asset) =>
      asset.type === context.expectedAssetType &&
      (context.ownerEntityId === null || asset.ownerEntityId === context.ownerEntityId),
  );
  const targetOptions = extraAsset ? [extraAsset, ...eligibleAssets] : eligibleAssets;
  const selected = artifacts.find((detail) => detail.artifact.id === selectedId);
  const targetAsset = targetOptions.find((asset) => asset.id === selectedTargetId) ?? targetOptions[0] ?? null;
  const blockedReason = !selected
    ? "Select a candidate result first."
    : !targetAsset
      ? `Create or select a ${operationLabel.toLowerCase()} asset to save into.`
      : null;

  async function handleCreateAsset() {
    setCreatePending(true);
    setCreateError(null);
    try {
      const name = createName.trim() || `New ${operationLabel}`;
      const created = await createAsset({
        projectRootPath,
        type: context.expectedAssetType,
        label: name,
        ownerEntityId: context.ownerEntityId,
      });
      setExtraAsset({
        id: created.id,
        projectId: created.projectId,
        type: created.type,
        label: created.label,
        ownerEntityId: created.ownerEntityId,
        canonicalVersionId: created.canonicalVersionId,
        versionCount: 0,
        canonicalVersionNumber: null,
        previewThumbnailPath: null,
        createdAt: created.createdAt,
        updatedAt: created.updatedAt,
      });
      setSelectedTargetId(created.id);
      setCreating(false);
      setCreateName("");
    } catch (reason) {
      setCreateError(describeError(reason));
    } finally {
      setCreatePending(false);
    }
  }

  return (
    <section className="generation-results" aria-labelledby="generation-results-title">
      <header className="production-panel-header">
        <div>
          <span className="production-kicker">Candidate set</span>
          <h2 id="generation-results-title">{operationLabel} Results</h2>
          <p>{artifacts.length} candidates generated from the pinned source references.</p>
        </div>
      </header>
      <div className="generation-result-grid">
        {artifacts.map((detail) => (
          <GenerationResultCard
            key={detail.artifact.id}
            projectRootPath={projectRootPath}
            detail={detail}
            selected={detail.artifact.id === selectedId}
            onSelect={() => setSelectedId(detail.artifact.id)}
          />
        ))}
      </div>
      {eligibleAssets.length || extraAsset ? (
        <label className="generation-target-row">
          Target asset
          <select
            value={targetAsset?.id ?? ""}
            onChange={(event) => setSelectedTargetId(event.target.value || null)}
          >
            {targetOptions.map((asset) => (
              <option key={asset.id} value={asset.id}>
                {asset.label}
                {asset.canonicalVersionNumber ? ` — v${String(asset.canonicalVersionNumber).padStart(3, "0")}` : ""}
              </option>
            ))}
          </select>
        </label>
      ) : (
        <div className="generation-target-row">
          {creating ? (
            <div className="generation-create-asset">
              <label htmlFor="generation-create-name">Asset name</label>
              <input
                id="generation-create-name"
                value={createName}
                onChange={(event) => setCreateName(event.target.value)}
                placeholder={`New ${operationLabel.toLowerCase()}`}
                disabled={createPending}
              />
              {createError ? <p role="alert">{createError}</p> : null}
              <div className="production-form-actions">
                <button type="button" onClick={() => void handleCreateAsset()} disabled={createPending}>
                  {createPending ? "Creating…" : "Create"}
                </button>
                <button type="button" className="production-secondary" onClick={() => setCreating(false)} disabled={createPending}>
                  Cancel
                </button>
              </div>
            </div>
          ) : (
            <button type="button" onClick={() => setCreating(true)}>
              Create asset
            </button>
          )}
        </div>
      )}
      <div className="generation-results-actions">
        <ActionButton disabled={!selected || !targetAsset} disabledReason={blockedReason} onClick={() => setPromoting(true)}>
          Save as Asset Version
        </ActionButton>
      </div>
      {promoting && selected && targetAsset ? (
        <PromoteArtifactDialog
          projectRootPath={projectRootPath}
          artifactId={selected.artifact.id}
          targetAsset={targetAsset}
          onClose={() => setPromoting(false)}
          onPromoted={(version: AssetVersion) => {
            setPromoting(false);
            onPromoted?.(targetAsset.id, version.id);
          }}
        />
      ) : null}
    </section>
  );
}
