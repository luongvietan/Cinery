import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { describeError } from "../../lib/errors";
import { importAssetVersion } from "./api";

interface ImportAssetVersionButtonProps {
  projectRootPath: string;
  assetId: string;
  /** Called after a successful import so the parent can refetch the asset. */
  onImported: () => void;
  className?: string;
}

export function ImportAssetVersionButton({
  projectRootPath,
  assetId,
  onImported,
  className,
}: ImportAssetVersionButtonProps) {
  const [error, setError] = useState<string | null>(null);
  const [importing, setImporting] = useState(false);

  async function handleClick() {
    setError(null);
    const selected = await open({
      multiple: false,
      filters: [
        {
          name: "Images",
          extensions: ["png", "jpg", "jpeg", "webp"],
        },
      ],
    });
    const sourcePath = typeof selected === "string" ? selected : null;
    if (!sourcePath) {
      return;
    }

    setImporting(true);
    try {
      await importAssetVersion({ projectRootPath, assetId, sourcePath });
      onImported();
    } catch (err) {
      setError(describeError(err));
    } finally {
      setImporting(false);
    }
  }

  return (
    <div className={className}>
      {error ? <p role="alert">{error}</p> : null}
      <button type="button" onClick={handleClick} disabled={importing}>
        {importing ? "Importing…" : "Import Version"}
      </button>
    </div>
  );
}
