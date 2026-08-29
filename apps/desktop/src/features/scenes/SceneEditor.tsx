import { useEffect, useState } from "react";
import { describeError } from "../../lib/errors";
import { getScene, updateSceneDetails } from "./api";
import type { Scene } from "./types";
import { formatSceneOrdinal } from "./types";

interface SceneEditorProps {
  projectRootPath: string;
  sceneId: string;
  onUpdated?: (scene: Scene) => void;
  onBack?: () => void;
}

export function SceneEditor({
  projectRootPath,
  sceneId,
  onUpdated,
  onBack,
}: SceneEditorProps) {
  const [scene, setScene] = useState<Scene | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [title, setTitle] = useState("");
  const [summary, setSummary] = useState("");
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [saveSuccess, setSaveSuccess] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    setSaveError(null);
    setSaveSuccess(null);
    getScene(projectRootPath, sceneId)
      .then((result) => {
        if (!cancelled) {
          setScene(result);
          setTitle(result.title);
          setSummary(result.summary);
        }
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
  }, [projectRootPath, sceneId]);

  async function handleSave(event: React.FormEvent) {
    event.preventDefault();
    if (!scene) return;
    const trimmedTitle = title.trim();
    if (!trimmedTitle) {
      setSaveError("Title must not be empty");
      return;
    }
    setSaving(true);
    setSaveError(null);
    setSaveSuccess(null);
    try {
      const updated = await updateSceneDetails(
        projectRootPath,
        sceneId,
        trimmedTitle,
        summary,
      );
      setScene(updated);
      setTitle(updated.title);
      setSummary(updated.summary);
      setSaveSuccess("Scene saved");
      onUpdated?.(updated);
    } catch (caught: unknown) {
      setSaveError(describeError(caught));
    } finally {
      setSaving(false);
    }
  }

  if (loading) {
    return <p role="status">Loading scene…</p>;
  }

  if (error) {
    return <p role="alert">{error}</p>;
  }

  if (!scene) {
    return <p role="alert">Scene not found.</p>;
  }

  return (
    <section aria-label="Scene editor" className="canon-content">
      {onBack ? (
        <button
          type="button"
          className="back-button"
          onClick={onBack}
          style={{ marginBottom: "var(--space-12)" }}
        >
          ← Scenes
        </button>
      ) : null}
      <header className="canon-panel-header">
        <div>
          <h2 style={{ textTransform: "uppercase" }}>
            {formatSceneOrdinal(scene.ordinal)} {scene.title}
          </h2>
          <p>Scene id {scene.id} · ordinal {scene.ordinal}</p>
        </div>
      </header>

      {saveError ? <p role="alert">{saveError}</p> : null}
      {saveSuccess ? <p role="status">{saveSuccess}</p> : null}

      <form
        onSubmit={handleSave}
        className="canon-create-form scene-editor-form"
        aria-label="Scene details"
        style={{
          maxWidth: "100%",
          display: "flex",
          flexDirection: "column",
          gap: "var(--space-12)",
        }}
      >
        <div
          className="scene-editor-grid"
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(auto-fit, minmax(280px, 1fr))",
            gap: "var(--space-16)",
          }}
        >
          <label htmlFor="scene-title">
            Title
            <input
              id="scene-title"
              value={title}
              onChange={(event) => setTitle(event.target.value)}
              placeholder="Scene title"
              required
            />
          </label>
          <label htmlFor="scene-summary">
            Summary
            <textarea
              id="scene-summary"
              value={summary}
              onChange={(event) => setSummary(event.target.value)}
              placeholder="Brief summary of the scene…"
              rows={4}
              style={{
                width: "100%",
                minHeight: "90px",
                fontFamily: "inherit",
                padding: "var(--space-8) var(--space-12)",
                border: "1px solid var(--c-hairline)",
                borderRadius: "var(--radius-md)",
                background: "var(--c-btn-bg)",
              }}
            />
          </label>
        </div>
        <div style={{ display: "flex", gap: "var(--space-8)" }}>
          <button type="submit" disabled={saving}>
            {saving ? "Saving…" : "Save Scene"}
          </button>
          <button
            type="button"
            className="canon-secondary-button"
            onClick={() => {
              if (scene) {
                setTitle(scene.title);
                setSummary(scene.summary);
                setSaveError(null);
                setSaveSuccess(null);
              }
            }}
          >
            Reset
          </button>
        </div>
        <p style={{ fontSize: "var(--fs-sm)", color: "var(--c-muted)" }}>
          Title is required. Summary may be empty during editing but readiness
          requires both.
        </p>
      </form>
    </section>
  );
}
