import { convertFileSrc } from "@tauri-apps/api/core";
import { type FormEvent, useEffect, useState } from "react";
import type { AssetSummary, AssetType } from "@cinematic/domain";
import { describeError } from "../../lib/errors";
import { createAsset, listAssets } from "./api";
import {
  describeAssetListStatus,
  humanizeAssetType,
  SPRINT_ONE_ASSET_TYPES,
} from "./format";
import { joinProjectRelativePath } from "./paths";
import { BackButton } from "../../components/BackButton";

interface AssetListProps {
  projectRootPath: string;
  selectedAssetId: string | null;
  refreshKey?: number;
  onSelectAsset: (assetId: string) => void;
  onBack?: () => void;
}

export function AssetList({
  projectRootPath,
  selectedAssetId,
  refreshKey = 0,
  onSelectAsset,
  onBack,
}: AssetListProps) {
  const [assets, setAssets] = useState<AssetSummary[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [isCreateOpen, setIsCreateOpen] = useState(false);
  const [assetType, setAssetType] = useState<AssetType>(
    SPRINT_ONE_ASSET_TYPES[0],
  );
  const [label, setLabel] = useState("");
  const [creating, setCreating] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setLoaded(false);

    listAssets(projectRootPath)
      .then((result) => {
        if (!cancelled) {
          setAssets(result);
        }
      })
      .catch((err: unknown) => {
        if (!cancelled) {
          setError(describeError(err));
        }
      })
      .finally(() => {
        if (!cancelled) {
          setLoaded(true);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [projectRootPath, refreshKey]);

  function handleNewAssetClick() {
    setError(null);
    setIsCreateOpen(true);
  }

  function handleCreateCancel() {
    setIsCreateOpen(false);
    setLabel("");
  }

  async function handleCreateSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setCreating(true);
    setError(null);
    try {
      const created = await createAsset({
        projectRootPath,
        type: assetType,
        label,
      });
      const refreshed = await listAssets(projectRootPath);
      setAssets(refreshed);
      setIsCreateOpen(false);
      setLabel("");
      onSelectAsset(created.id);
    } catch (err) {
      setError(describeError(err));
    } finally {
      setCreating(false);
    }
  }

  return (
    <section aria-label="Assets">
      {onBack ? <BackButton label="← Workspace" onClick={onBack} /> : null}
      {error ? <p role="alert">{error}</p> : null}

      <button
        type="button"
        onClick={handleNewAssetClick}
        disabled={isCreateOpen}
      >
        New Asset
      </button>

      {isCreateOpen ? (
        <form onSubmit={handleCreateSubmit}>
          <label htmlFor="new-asset-type">Type</label>
          <select
            id="new-asset-type"
            value={assetType}
            onChange={(event) => setAssetType(event.target.value as AssetType)}
          >
            {SPRINT_ONE_ASSET_TYPES.map((type) => (
              <option key={type} value={type}>
                {humanizeAssetType(type)}
              </option>
            ))}
          </select>

          <label htmlFor="new-asset-label">Label</label>
          <input
            id="new-asset-label"
            value={label}
            onChange={(event) => setLabel(event.target.value)}
            required
          />

          <button type="submit" disabled={creating}>
            Create
          </button>
          <button
            type="button"
            onClick={handleCreateCancel}
            disabled={creating}
          >
            Cancel
          </button>
        </form>
      ) : null}

      {loaded && !error && assets.length === 0 ? (
        <p>No assets yet</p>
      ) : (
        <ul className="asset-sidebar-list">
          {assets.map((asset) => {
            const statusLabel = describeAssetListStatus(asset);
            const hasCanonical = asset.canonicalVersionNumber !== null;
            const hasVersions = asset.versionCount > 0;
            const isSelected = asset.id === selectedAssetId;

            return (
              <li key={asset.id}>
                <button
                  type="button"
                  className={
                    isSelected
                      ? "asset-sidebar-item asset-sidebar-item--selected"
                      : "asset-sidebar-item"
                  }
                  onClick={() => onSelectAsset(asset.id)}
                >
                  {asset.previewThumbnailPath ? (
                    <img
                      className="asset-sidebar-thumb"
                      src={convertFileSrc(
                        joinProjectRelativePath(
                          projectRootPath,
                          asset.previewThumbnailPath,
                        ),
                      )}
                      alt=""
                    />
                  ) : (
                    <span
                      className="asset-sidebar-thumb asset-sidebar-thumb--empty"
                      aria-hidden="true"
                    />
                  )}
                  <span className="asset-sidebar-copy">
                    <span className="asset-sidebar-label">{asset.label}</span>
                    <span className="asset-sidebar-type">
                      {humanizeAssetType(asset.type)}
                    </span>
                    <span
                      className={
                        hasCanonical
                          ? "asset-sidebar-status asset-sidebar-status--canonical"
                          : "asset-sidebar-status"
                      }
                    >
                      <span aria-hidden="true">
                        {hasVersions ? (hasCanonical ? "●" : "○") : "○"}
                      </span>{" "}
                      {statusLabel}
                    </span>
                  </span>
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}
