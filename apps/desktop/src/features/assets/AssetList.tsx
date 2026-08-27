import { type FormEvent, useEffect, useState } from "react";
import type { Asset, AssetType } from "@cinematic/domain";
import { createAsset, listAssets } from "./api";
import { describeCommandError } from "./errors";
import { humanizeAssetType, SPRINT_ONE_ASSET_TYPES } from "./format";

interface AssetListProps {
  projectRootPath: string;
  onSelectAsset: (assetId: string) => void;
}

export function AssetList({ projectRootPath, onSelectAsset }: AssetListProps) {
  const [assets, setAssets] = useState<Asset[]>([]);
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

    refreshAssets(projectRootPath)
      .then((result) => {
        if (!cancelled) {
          setAssets(result);
        }
      })
      .catch((err: unknown) => {
        if (!cancelled) {
          setError(describeCommandError(err));
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
  }, [projectRootPath]);

  function refreshAssets(rootPath: string): Promise<Asset[]> {
    return listAssets(rootPath);
  }

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
      const refreshed = await refreshAssets(projectRootPath);
      setAssets(refreshed);
      setIsCreateOpen(false);
      setLabel("");
      onSelectAsset(created.id);
    } catch (err) {
      setError(describeCommandError(err));
    } finally {
      setCreating(false);
    }
  }

  return (
    <section aria-label="Assets">
      {error && <p role="alert">{error}</p>}

      <button
        type="button"
        onClick={handleNewAssetClick}
        disabled={isCreateOpen}
      >
        New Asset
      </button>

      {isCreateOpen && (
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
      )}

      {loaded && assets.length === 0 ? (
        <p>No assets yet</p>
      ) : (
        <ul>
          {assets.map((asset) => (
            <li key={asset.id}>
              <button type="button" onClick={() => onSelectAsset(asset.id)}>
                <span>{asset.label}</span>
                <span>{humanizeAssetType(asset.type)}</span>
                <span>
                  {asset.canonicalVersionId
                    ? "Canonical set"
                    : "No canonical version"}
                </span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
