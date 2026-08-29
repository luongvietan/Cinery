import { useEffect, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import {
  formatVersionNumber,
  type GeneratedArtifactDetail,
  type AssetVersion,
  type AssetWithVersions,
} from "@cinematic/domain";
import { describeError } from "../../lib/errors";
import { getAssetWithVersions, promoteAssetVersion } from "./api";
import { getGeneratedArtifact } from "../production/api";
import { QaPanel } from "../qa/QaPanel";
import { ProvenancePanel } from "../provenance/ProvenancePanel";
import {
  formatByteSize,
  formatImageDimensions,
  formatImageFormat,
  formatImportedDate,
  formatStatusLabel,
  humanizeAssetType,
} from "./format";
import { ImportAssetVersionButton } from "./ImportAssetVersionButton";
import { joinProjectRelativePath } from "./paths";
import { BackButton } from "../../components/BackButton";
import {
  openAssetFolder,
  openProjectRelativePath,
  revealProjectRelativePath,
} from "./shell";

interface AssetInspectorProps {
  projectRootPath: string;
  assetId: string;
  onAssetChanged?: () => void;
  onBack?: () => void;
}

function versionThumbnailSrc(
  projectRootPath: string,
  version: AssetVersion,
): string {
  return convertFileSrc(
    joinProjectRelativePath(projectRootPath, version.thumbnailPath),
  );
}

function versionPreviewSrc(
  projectRootPath: string,
  version: AssetVersion,
): string {
  return convertFileSrc(
    joinProjectRelativePath(projectRootPath, version.filePath),
  );
}

export function AssetInspector({
  projectRootPath,
  assetId,
  onAssetChanged,
  onBack,
}: AssetInspectorProps) {
  const [data, setData] = useState<AssetWithVersions | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [promotionError, setPromotionError] = useState<string | null>(null);
  const [promotingVersionId, setPromotingVersionId] = useState<string | null>(
    null,
  );
  const [generationDetails, setGenerationDetails] = useState<GeneratedArtifactDetail | null>(null);
  const [generationDetailsError, setGenerationDetailsError] = useState<string | null>(null);
  const [qaVersionId, setQaVersionId] = useState<string | null>(null);
  const [provenanceVersionId, setProvenanceVersionId] = useState<string | null>(null);
  const selectionKey = `${projectRootPath}\u0000${assetId}`;
  const currentSelectionKey = useRef(selectionKey);
  currentSelectionKey.current = selectionKey;

  async function refreshAsset() {
    const result = await getAssetWithVersions(projectRootPath, assetId);
    if (currentSelectionKey.current === selectionKey) {
      setData(result);
    }
    return result;
  }

  useEffect(() => {
    let cancelled = false;
    setData(null);
    setError(null);
    setPromotionError(null);
    setPromotingVersionId(null);
    setQaVersionId(null);
    setProvenanceVersionId(null);

    getAssetWithVersions(projectRootPath, assetId)
      .then((result) => {
        if (!cancelled) {
          setData(result);
        }
      })
      .catch((err: unknown) => {
        if (!cancelled) {
          setError(describeError(err));
        }
      });

    return () => {
      cancelled = true;
    };
  }, [projectRootPath, assetId]);

  async function handleImported() {
    try {
      await refreshAsset();
      onAssetChanged?.();
    } catch (err: unknown) {
      setError(describeError(err));
    }
  }

  async function handlePromote(
    assetVersionId: string,
    versionNumber: number,
  ) {
    if (!data) {
      return;
    }

    const versionLabel = formatVersionNumber(versionNumber);
    const confirmationDetail = data.asset.canonicalVersionId
      ? "The current canonical version will be preserved and marked Superseded."
      : "This version will become the asset's canonical version.";
    const confirmed = window.confirm(
      `Make ${versionLabel} the canonical version of ${data.asset.label}?\n${confirmationDetail}`,
    );
    if (!confirmed) {
      return;
    }

    const requestSelectionKey = selectionKey;
    setPromotionError(null);
    setPromotingVersionId(assetVersionId);

    try {
      await promoteAssetVersion({
        projectRootPath,
        assetVersionId,
      });
      await refreshAsset();
      onAssetChanged?.();
    } catch (err: unknown) {
      if (currentSelectionKey.current === requestSelectionKey) {
        setPromotionError(describeError(err));
      }
    } finally {
      if (currentSelectionKey.current === requestSelectionKey) {
        setPromotingVersionId(null);
      }
    }
  }

  async function handleGenerationDetails(artifactId: string) {
    setGenerationDetailsError(null);
    try {
      setGenerationDetails(await getGeneratedArtifact(projectRootPath, artifactId));
    } catch (err: unknown) {
      setGenerationDetailsError(describeError(err));
    }
  }

  if (error) {
    return <p role="alert">{error}</p>;
  }

  if (!data) {
    return <p>Loading asset…</p>;
  }

  const { asset, versions } = data;
  const sortedVersions = [...versions].sort(
    (a, b) => b.versionNumber - a.versionNumber,
  );
  const canonicalVersion =
    sortedVersions.find((version) => version.id === asset.canonicalVersionId) ??
    null;
  const canonicalLabel = canonicalVersion
    ? formatVersionNumber(canonicalVersion.versionNumber)
    : versions.length > 0
      ? "No canonical version"
      : "No versions";
  const qaVersion =
    sortedVersions.find((version) => version.id === qaVersionId) ??
    sortedVersions[0] ??
    null;

  return (
    <section aria-label={`Asset ${asset.label}`}>
      {onBack ? (
        <BackButton label="← Assets" onClick={onBack} />
      ) : null}
      <header className="asset-inspector-header">
        <div className="asset-inspector-title">
          <h2>{asset.label}</h2>
          <p>{humanizeAssetType(asset.type)}</p>
          <p className="asset-inspector-canonical-line">
            Canonical: {canonicalLabel}
          </p>
        </div>
        <div className="asset-inspector-actions">
          <ImportAssetVersionButton
            projectRootPath={projectRootPath}
            assetId={assetId}
            onImported={() => {
              void handleImported();
            }}
            className="asset-import-action"
          />
          <button
            type="button"
            className="asset-secondary-button"
            onClick={() => {
              void openAssetFolder(projectRootPath, assetId);
            }}
          >
            Open Folder
          </button>
        </div>
      </header>

      {promotionError ? <p role="alert">{promotionError}</p> : null}

      {canonicalVersion ? (
        <section
          aria-label="Canonical Version"
          className="asset-canonical-panel"
        >
          <h3>Canonical Version</h3>
          <article className="asset-canonical-card">
            <img
              className="asset-canonical-preview"
              src={versionPreviewSrc(projectRootPath, canonicalVersion)}
              alt={`${asset.label} ${formatVersionNumber(canonicalVersion.versionNumber)} preview`}
            />
            <div className="asset-canonical-meta">
              <p className="asset-version-label">
                {formatVersionNumber(canonicalVersion.versionNumber)}
              </p>
              <p className="asset-version-badge asset-version-badge--canonical">
                Canonical
              </p>
              <p>
                Imported: {formatImportedDate(canonicalVersion.createdAt)}
              </p>
              <p>
                {formatImageDimensions(
                  canonicalVersion.width,
                  canonicalVersion.height,
                )}{" "}
                · {formatImageFormat(canonicalVersion.mimeType)}
              </p>
              <p>{formatByteSize(canonicalVersion.byteSize)}</p>
            </div>
          </article>
        </section>
      ) : null}

      <section aria-label="Versions" className="asset-versions-panel">
        <h3>Versions</h3>
        {sortedVersions.length === 0 ? (
          <p className="asset-empty-state">
            No versions yet. Import an image to create the first version.
          </p>
        ) : (
          <ul className="asset-version-list">
            {sortedVersions.map((version) => {
              const isCanonical = version.id === asset.canonicalVersionId;
              return (
                <li
                  key={version.id}
                  data-testid="asset-version"
                  className={
                    isCanonical
                      ? "asset-version-card asset-version-card--canonical"
                      : "asset-version-card"
                  }
                >
                  <div className="asset-version-card-header">
                    <span className="asset-version-label">
                      {formatVersionNumber(version.versionNumber)}
                    </span>
                    {isCanonical ? (
                      <span className="asset-version-badge asset-version-badge--canonical">
                        Canonical
                      </span>
                    ) : null}
                  </div>
                  <div className="asset-version-card-body">
                    <img
                      src={versionThumbnailSrc(projectRootPath, version)}
                      alt={`${asset.label} ${formatVersionNumber(version.versionNumber)} thumbnail`}
                    />
                    <div className="asset-version-details">
                      <p>{formatStatusLabel(version.status)}</p>
                      {version.origin === "generated" ? (
                        <p className="asset-version-badge asset-version-badge--generated">
                          GENERATED
                        </p>
                      ) : null}
                      <p>{version.originalFilename}</p>
                      <p>
                        {formatImageDimensions(version.width, version.height)} ·{" "}
                        {formatImageFormat(version.mimeType)} ·{" "}
                        {formatByteSize(version.byteSize)}
                      </p>
                      <p>Imported: {formatImportedDate(version.createdAt)}</p>
                    </div>
                  </div>
                  <div className="asset-version-actions">
                    {!isCanonical ? (
                      <button
                        type="button"
                        className="asset-secondary-button"
                        disabled={promotingVersionId !== null}
                        onClick={() => {
                          void handlePromote(version.id, version.versionNumber);
                        }}
                      >
                        {promotingVersionId === version.id
                          ? "Setting canonical…"
                          : "Set Canonical"}
                      </button>
                    ) : null}
                    <button
                      type="button"
                      className="asset-secondary-button"
                      onClick={() => {
                        void openProjectRelativePath(
                          projectRootPath,
                          version.filePath,
                        );
                      }}
                    >
                      Open
                    </button>
                    <button
                      type="button"
                      className="asset-secondary-button"
                      onClick={() => {
                        void revealProjectRelativePath(
                          projectRootPath,
                          version.filePath,
                        );
                      }}
                    >
                      Reveal
                    </button>
                    {version.origin === "generated" && version.generationArtifactId ? (
                      <button
                        type="button"
                        className="asset-secondary-button"
                        onClick={() => {
                          void handleGenerationDetails(version.generationArtifactId!);
                        }}
                      >
                        View generation details
                      </button>
                    ) : null}
                    <button
                      type="button"
                      className="asset-secondary-button"
                      aria-pressed={qaVersion?.id === version.id}
                      onClick={() => setQaVersionId(version.id)}
                    >
                      View QA
                    </button>
                    <button
                      type="button"
                      className="asset-secondary-button"
                      aria-pressed={provenanceVersionId === version.id}
                      onClick={() =>
                        setProvenanceVersionId((current) =>
                          current === version.id ? null : version.id,
                        )
                      }
                    >
                      View provenance
                    </button>
                  </div>
                </li>
              );
            })}
          </ul>
        )}
      </section>
      {qaVersion ? (
        <QaPanel
          projectRootPath={projectRootPath}
          assetVersionId={qaVersion.id}
          versionLabel={formatVersionNumber(qaVersion.versionNumber)}
        />
      ) : null}
      {provenanceVersionId ? (
        <ProvenancePanel
          projectRootPath={projectRootPath}
          targetKind="asset_version"
          targetId={provenanceVersionId}
        />
      ) : null}
      {generationDetailsError ? <p role="alert">{generationDetailsError}</p> : null}
      {generationDetails ? (
        <section aria-label="Generation Details" className="asset-generation-details">
          <h3>Generation Details</h3>
          <dl>
            <div><dt>Workflow</dt><dd>{generationDetails.lineage?.workflowDefinitionId ?? "Unavailable"} · {generationDetails.lineage?.workflowVersion ?? ""}</dd></div>
            <div><dt>Skill</dt><dd>{generationDetails.lineage?.skillId ?? "Unavailable"} · {generationDetails.lineage?.skillVersion ?? ""}</dd></div>
            <div><dt>Canon snapshot</dt><dd>{generationDetails.lineage?.canonSnapshotId ?? "None"}</dd></div>
            <div><dt>Provider</dt><dd>{generationDetails.lineage?.providerId ?? "Unavailable"} · {generationDetails.lineage?.modelId ?? ""}</dd></div>
            <div><dt>Provider attempt</dt><dd>{generationDetails.lineage?.providerAttemptId ?? "Unavailable"}</dd></div>
            <div><dt>Source version(s)</dt><dd>{generationDetails.lineage?.sourceAssetVersionIds.join(", ") ?? "Unavailable"}</dd></div>
            <div><dt>Artifact SHA-256</dt><dd>{generationDetails.artifact.sha256}</dd></div>
          </dl>
        </section>
      ) : null}
    </section>
  );
}
