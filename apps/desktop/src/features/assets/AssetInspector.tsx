import { useEffect, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { formatVersionNumber, type AssetWithVersions } from "@cinematic/domain";
import { describeError } from "../../lib/errors";
import { getAssetWithVersions } from "./api";
import { formatStatusLabel, humanizeAssetType } from "./format";
import { joinProjectRelativePath } from "./paths";

interface AssetInspectorProps {
  projectRootPath: string;
  assetId: string;
}

export function AssetInspector({
  projectRootPath,
  assetId,
}: AssetInspectorProps) {
  const [data, setData] = useState<AssetWithVersions | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setData(null);
    setError(null);

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

  if (error) {
    return <p role="alert">{error}</p>;
  }

  if (!data) {
    return <p>Loading asset…</p>;
  }

  const { asset, versions } = data;

  // The backend already returns versions ordered `version_number DESC`
  // (see `assets/repository.rs::list_asset_versions`), but we sort
  // defensively here so the newest-first guarantee doesn't silently depend
  // on that implementation detail.
  const sortedVersions = [...versions].sort(
    (a, b) => b.versionNumber - a.versionNumber,
  );

  return (
    <section aria-label={`Asset ${asset.label}`}>
      <header>
        <h2>{asset.label}</h2>
        <p>{humanizeAssetType(asset.type)}</p>
        <p>
          {asset.canonicalVersionId
            ? `Canonical version: ${asset.canonicalVersionId}`
            : "No canonical version"}
        </p>
      </header>
      <ul>
        {sortedVersions.map((version) => (
          <li key={version.id} data-testid="asset-version">
            <img
              src={convertFileSrc(
                joinProjectRelativePath(projectRootPath, version.thumbnailPath),
              )}
              alt={`${asset.label} ${formatVersionNumber(version.versionNumber)} thumbnail`}
            />
            <dl>
              <dt>Version</dt>
              <dd>{formatVersionNumber(version.versionNumber)}</dd>
              <dt>Status</dt>
              <dd>{formatStatusLabel(version.status)}</dd>
              <dt>Original filename</dt>
              <dd>{version.originalFilename}</dd>
              <dt>MIME type</dt>
              <dd>{version.mimeType}</dd>
              <dt>Size</dt>
              <dd>{version.byteSize} bytes</dd>
              <dt>SHA-256</dt>
              <dd>{version.sha256}</dd>
              <dt>File path</dt>
              <dd>{version.filePath}</dd>
              <dt>Imported</dt>
              <dd>
                <time dateTime={version.createdAt}>{version.createdAt}</time>
              </dd>
            </dl>
          </li>
        ))}
      </ul>
    </section>
  );
}
