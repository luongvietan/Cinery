import { useEffect, useState } from "react";
import { describeError } from "../../lib/errors";
import {
  compileCinema,
  getCompileReadiness,
  listCinemaCompilations,
  listShots,
  type CinemaCompilation,
  type CompileReadiness,
} from "./api";

interface SceneCompileProps {
  projectRootPath: string;
  sceneId: string;
  onChanged?: () => void;
}

/**
 * Compile/export section for the authoritative Scene: readiness blockers,
 * the compile action over the scene's shots, and the persisted compilation
 * history with export artifacts.
 */
export function SceneCompile({ projectRootPath, sceneId, onChanged }: SceneCompileProps) {
  const [readiness, setReadiness] = useState<CompileReadiness | null>(null);
  const [compilations, setCompilations] = useState<CinemaCompilation[]>([]);
  const [totalDuration, setTotalDuration] = useState("8");
  const [compiling, setCompiling] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [lastCompilation, setLastCompilation] = useState<CinemaCompilation | null>(null);

  useEffect(() => {
    let cancelled = false;
    setError(null);
    setLastCompilation(null);
    Promise.all([
      getCompileReadiness(projectRootPath, sceneId),
      listCinemaCompilations(projectRootPath, sceneId),
      listShots(projectRootPath, sceneId),
    ])
      .then(([nextReadiness, nextCompilations, shots]) => {
        if (cancelled) return;
        setReadiness(nextReadiness);
        setCompilations(nextCompilations);
        const shotTotal = shots.reduce((sum, shot) => sum + shot.durationSeconds, 0);
        if (shotTotal > 0) {
          setTotalDuration(String(Math.min(120, Math.round(shotTotal * 100) / 100)));
        }
      })
      .catch((caught: unknown) => {
        if (!cancelled) setError(describeError(caught));
      });
    return () => {
      cancelled = true;
    };
  }, [projectRootPath, sceneId]);

  async function handleCompile() {
    const seconds = Number(totalDuration);
    if (!Number.isFinite(seconds) || seconds < 1 || seconds > 120) {
      setError("Total duration must be between 1 and 120 seconds");
      return;
    }
    setCompiling(true);
    setError(null);
    try {
      const compilation = await compileCinema(projectRootPath, sceneId, seconds);
      setLastCompilation(compilation);
      setCompilations(
        await listCinemaCompilations(projectRootPath, sceneId),
      );
      setReadiness(await getCompileReadiness(projectRootPath, sceneId));
      onChanged?.();
    } catch (caught: unknown) {
      setError(describeError(caught));
    } finally {
      setCompiling(false);
    }
  }

  return (
    <section
      aria-label="Scene compile"
      style={{ padding: "16px", background: "var(--surface-card)", border: "1px solid var(--color-hairline)", borderRadius: "10px" }}
    >
      <header>
        <h3 style={{ margin: 0, textTransform: "uppercase", fontSize: "13px", letterSpacing: "0.04em" }}>COMPILE / EXPORT</h3>
        <p style={{ margin: "4px 0 0", fontSize: "13px", color: "var(--color-mid-gray)" }}>
          Compile the scene into a deterministic provider-neutral production prompt.
        </p>
      </header>

      {error ? <p role="alert">{error}</p> : null}

      {readiness && !readiness.ready ? (
        <div role="status" style={{ margin: "12px 0" }}>
          <p style={{ margin: "0 0 4px", fontWeight: 600 }}>Not ready to compile:</p>
          <ul style={{ margin: 0, paddingLeft: "20px" }}>
            {readiness.blockers.map((blocker) => (
              <li key={`${blocker.code}-${blocker.shotId ?? blocker.entityId ?? "scene"}`}>{blocker.message}</li>
            ))}
          </ul>
        </div>
      ) : null}

      <div style={{ display: "flex", gap: "8px", alignItems: "end", flexWrap: "wrap", marginTop: "12px" }}>
        <label htmlFor="compile-duration">
          Total runtime (s)
          <input
            id="compile-duration"
            type="number"
            min="1"
            max="120"
            step="0.5"
            value={totalDuration}
            onChange={(event) => setTotalDuration(event.target.value)}
          />
        </label>
        <button
          type="button"
          onClick={() => void handleCompile()}
          disabled={compiling || (readiness ? !readiness.ready : false)}
          title={readiness && !readiness.ready ? "Resolve the readiness blockers first" : undefined}
        >
          {compiling ? "Compiling…" : "Compile Scene"}
        </button>
      </div>

      {lastCompilation ? (
        <div style={{ marginTop: "12px", fontSize: "13px" }}>
          <p style={{ margin: "0 0 4px", fontWeight: 600 }}>Latest compilation</p>
          <p style={{ margin: 0 }}>Export: {lastCompilation.exportPath}</p>
          <p style={{ margin: 0 }}>SHA-256: {lastCompilation.exportSha256}</p>
        </div>
      ) : null}

      {compilations.length > 0 ? (
        <div style={{ marginTop: "12px" }}>
          <h4 style={{ margin: "0 0 4px" }}>Compilation history</h4>
          <ul style={{ margin: 0, paddingLeft: "20px", fontSize: "13px" }}>
            {compilations.map((compilation) => (
              <li key={compilation.id}>
                {new Date(compilation.createdAt).toLocaleString()} — {compilation.exportPath} (sha {compilation.exportSha256.slice(0, 12)}…)
              </li>
            ))}
          </ul>
        </div>
      ) : null}
    </section>
  );
}
