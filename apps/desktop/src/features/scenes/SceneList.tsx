import { useEffect, useState } from "react";
import { describeError } from "../../lib/errors";
import { listScenes } from "./api";
import { formatSceneOrdinal } from "./types";
import type { Scene } from "./types";

interface SceneListProps {
  projectRootPath: string;
  selectedSceneId: string | null;
  onSelectScene: (sceneId: string) => void;
  refreshKey?: number;
}

function readinessLabel(scene: Scene): string {
  if (!scene.title.trim() || !scene.summary.trim()) {
    return "DRAFT";
  }
  if (!scene.worldId || !scene.worldAssetVersionId) {
    return "NEEDS WORLD";
  }
  return "READY";
}

export function SceneList({
  projectRootPath,
  selectedSceneId,
  onSelectScene,
  refreshKey = 0,
}: SceneListProps) {
  const [scenes, setScenes] = useState<Scene[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    listScenes(projectRootPath)
      .then((result) => {
        if (!cancelled) setScenes(result ?? []);
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
    return <p role="status">Loading scenes…</p>;
  }

  if (error) {
    return <p role="alert">{error}</p>;
  }

  if (scenes.length === 0) {
    return (
      <div className="scene-list-empty" aria-label="Scenes">
        <p>No scenes yet</p>
        <p>Create a Scene to begin assembling your visual narrative.</p>
      </div>
    );
  }

  return (
    <section aria-label="Scenes">
      <ul className="canon-entity-list" role="list">
        {scenes.map((scene) => {
          const isSelected = scene.id === selectedSceneId;
          const ordinalLabel = formatSceneOrdinal(scene.ordinal);
          const readiness = readinessLabel(scene);
          return (
            <li key={scene.id}>
              <button
                type="button"
                className={
                  isSelected
                    ? "canon-entity-button canon-entity-button--selected"
                    : "canon-entity-button"
                }
                aria-pressed={isSelected}
                onClick={() => onSelectScene(scene.id)}
              >
                <span
                  style={{
                    display: "block",
                    fontWeight: 600,
                    textTransform: "uppercase",
                    fontSize: "12px",
                    color: "var(--color-mid-gray)",
                  }}
                >
                  {ordinalLabel}
                </span>
                <span
                  style={{
                    display: "block",
                    fontWeight: 600,
                  }}
                >
                  {scene.title}
                </span>
                <span
                  className={
                    readiness === "READY"
                      ? "canon-badge"
                      : "canon-status canon-status--draft"
                  }
                  style={{
                    display: "inline-block",
                    marginTop: "4px",
                    fontSize: "11px",
                  }}
                >
                  {readiness}
                </span>
              </button>
            </li>
          );
        })}
      </ul>
    </section>
  );
}
