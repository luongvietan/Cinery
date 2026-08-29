import { useEffect, useState } from "react";
import { describeError } from "../../lib/errors";
import { listWorldsDetailed } from "./api";
import type { WorldDetail } from "./types";

interface WorldListProps {
  projectRootPath: string;
  selectedWorldId: string | null;
  onSelectWorld: (worldId: string) => void;
  refreshKey?: number;
}

function worldPlateStatus(detail: WorldDetail): string {
  const asset = detail.worldPlateAsset;
  if (!asset.canonicalVersionId) {
    return "NO WORLD PLATE YET";
  }
  // If canonical exists, show label + canonical badge. Version number lookup would require extra fetch;
  // fallback to label-based display which still surfaces canonical state.
  return `${asset.label} · CANONICAL`;
}

function formatWorldListSecondary(detail: WorldDetail): string {
  const asset = detail.worldPlateAsset;
  if (!asset.canonicalVersionId) {
    return "No world plate yet";
  }
  return "World plate canonical";
}

export function WorldList({
  projectRootPath,
  selectedWorldId,
  onSelectWorld,
  refreshKey = 0,
}: WorldListProps) {
  const [worlds, setWorlds] = useState<WorldDetail[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    listWorldsDetailed(projectRootPath)
      .then((result) => {
        if (!cancelled) setWorlds(result ?? []);
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
  }, [projectRootPath, refreshKey]);

  if (loading) {
    return <p role="status">Loading worlds…</p>;
  }

  if (error) {
    return <p role="alert">{error}</p>;
  }

  if (worlds.length === 0) {
    return (
      <div className="world-list-empty" aria-label="Worlds">
        <p>No worlds yet</p>
        <p>A world is the backdrop your scenes happen in, generated from a location in your story. Create one to begin.</p>
      </div>
    );
  }

  return (
    <section aria-label="Worlds">
      <ul className="canon-entity-list" role="list">
        {worlds.map((detail) => {
          const isSelected = detail.world.id === selectedWorldId;
          return (
            <li key={detail.world.id}>
              <button
                type="button"
                className={
                  isSelected
                    ? "canon-entity-button canon-entity-button--selected"
                    : "canon-entity-button"
                }
                aria-pressed={isSelected}
                onClick={() => onSelectWorld(detail.world.id)}
              >
                <span
                  className="world-list-location"
                  style={{
                    display: "block",
                    fontWeight: 600,
                    textTransform: "uppercase",
                  }}
                >
                  {detail.location.name}
                </span>
                <span
                  className="world-list-status"
                  style={{
                    display: "block",
                    fontSize: "var(--fs-sm)",
                    color: "var(--c-muted)",
                  }}
                >
                  {worldPlateStatus(detail)}
                </span>
                <span
                  className="world-list-secondary"
                  style={{
                    display: "block",
                    fontSize: "var(--fs-sm)",
                    color: "var(--c-muted)",
                  }}
                >
                  {formatWorldListSecondary(detail)}
                  {detail.worldPlateAsset.canonicalVersionId
                    ? ` · ${detail.worldPlateAsset.canonicalVersionId}`
                    : null}
                </span>
              </button>
            </li>
          );
        })}
      </ul>
    </section>
  );
}
