import { useEffect, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import type { AssetWithVersions, WorkflowRunDetail } from "@cinematic/domain";
import { describeError } from "../../lib/errors";
import { getAssetWithVersions } from "../assets/api";
import { joinProjectRelativePath } from "../assets/paths";
import {
  advanceWorkflowRun,
} from "../workflows/api";
import { WorkflowRunView } from "../workflows/WorkflowRunView";
import { AssetInspector } from "../assets/AssetInspector";
import { createWorldPlateWorkflowRun } from "./api";
import type { WorldDetail } from "./types";
import { formatVersionNumber } from "@cinematic/domain";

interface WorldPlatePanelProps {
  projectRootPath: string;
  detail: WorldDetail;
}

export function WorldPlatePanel({
  projectRootPath,
  detail,
}: WorldPlatePanelProps) {
  const [assetData, setAssetData] = useState<AssetWithVersions | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [workflowRun, setWorkflowRun] = useState<WorkflowRunDetail | null>(
    null,
  );
  const [pending, setPending] = useState(false);
  const [showVersions, setShowVersions] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);

  const worldPlateAssetId = detail.worldPlateAsset.id;

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    setShowVersions(false);
    setWorkflowRun(null);
    getAssetWithVersions(projectRootPath, worldPlateAssetId)
      .then((result) => {
        if (!cancelled) setAssetData(result);
      })
      .catch((caught: unknown) => {
        if (!cancelled) setError(describeError(caught));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [projectRootPath, worldPlateAssetId]);

  async function handleGenerate() {
    setPending(true);
    setActionError(null);
    try {
      const created = await createWorldPlateWorkflowRun(
        projectRootPath,
        detail.world.id,
        [],
      );
      // advance to waiting_for_approval (first advance creates snapshot etc)
      const waiting = await advanceWorkflowRun(projectRootPath, created.run.id);
      setWorkflowRun(waiting);
    } catch (caught: unknown) {
      setActionError(describeError(caught));
    } finally {
      setPending(false);
    }
  }

  function handleRunChange(next: WorkflowRunDetail) {
    setWorkflowRun(next);
    // if completed, refresh asset data
    if (next.run.status === "completed") {
      void getAssetWithVersions(projectRootPath, worldPlateAssetId)
        .then(setAssetData)
        .catch(() => undefined);
    }
  }

  if (loading) {
    return <p role="status">Loading world plate…</p>;
  }

  if (error) {
    return <p role="alert">{error}</p>;
  }

  const canonicalVersion = assetData
    ? assetData.versions.find(
        (version) => version.id === assetData.asset.canonicalVersionId,
      ) ?? null
    : null;

  const hasCanonical = Boolean(canonicalVersion);

  return (
    <section aria-label="World Plate" className="world-plate-panel">
      <header className="canon-panel-header">
        <div>
          <h3>WORLD PLATE</h3>
          <p>
            Persistent environment truth. Stable asset{" "}
            <strong>{detail.worldPlateAsset.label}</strong>.
          </p>
        </div>
      </header>

      {actionError ? <p role="alert">{actionError}</p> : null}

      {/* Preview area */}
      <div
        className="world-plate-preview"
        style={{
          display: "grid",
          gap: "var(--space-12)",
          padding: "var(--space-16)",
          background: "var(--c-panel)",
          border: "1px solid var(--c-hairline)",
          borderRadius: "var(--radius-lg)",
        }}
      >
        {hasCanonical && canonicalVersion ? (
          <>
            <img
              src={convertFileSrc(
                joinProjectRelativePath(
                  projectRootPath,
                  canonicalVersion.filePath,
                ),
              )}
              alt={`${detail.worldPlateAsset.label} ${formatVersionNumber(canonicalVersion.versionNumber)} preview`}
              style={{
                width: "100%",
                maxHeight: "360px",
                objectFit: "contain",
                background: "var(--c-hairline)",
                borderRadius: "var(--radius-md)",
              }}
            />
            <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-4)" }}>
              <span style={{ fontWeight: 600 }}>
                {detail.worldPlateAsset.label}-{formatVersionNumber(canonicalVersion.versionNumber)}
              </span>
              <span
                className="asset-version-badge asset-version-badge--canonical"
                style={{ alignSelf: "flex-start" }}
              >
                CANONICAL
              </span>
            </div>
          </>
        ) : (
          <p>NO WORLD PLATE YET — generate a candidate to create the first version.</p>
        )}

        <div style={{ display: "flex", gap: "var(--space-8)", flexWrap: "wrap" }}>
          <button
            type="button"
            onClick={() => void handleGenerate()}
            disabled={pending}
          >
            {pending ? "Preparing…" : "Generate Candidate"}
          </button>
          <button
            type="button"
            className="canon-secondary-button"
            onClick={() => setShowVersions((value) => !value)}
          >
            {showVersions ? "Hide Versions" : "View Versions"}
          </button>
        </div>
      </div>

      {/* Version handling */}
      {showVersions ? (
        <div style={{ marginTop: "var(--space-16)" }}>
          <AssetInspector
            projectRootPath={projectRootPath}
            assetId={worldPlateAssetId}
            onAssetChanged={() => {
              void getAssetWithVersions(projectRootPath, worldPlateAssetId)
                .then(setAssetData)
                .catch(() => undefined);
            }}
          />
        </div>
      ) : null}

      {/* Workflow review: reuse existing WorkflowRunView, do not create second approval dialog */}
      {workflowRun ? (
        <div style={{ marginTop: "var(--space-16)" }}>
          <WorkflowRunView
            projectRootPath={projectRootPath}
            detail={workflowRun}
            onChange={handleRunChange}
          />
        </div>
      ) : null}
    </section>
  );
}
