import { useEffect, useState } from "react";
import { describeError } from "../../lib/errors";
import {
  getScene,
  listSceneCharacters,
  listSceneProps,
  listSceneTbdBindings,
  resolveSceneReferences,
} from "./api";
import { listCanonTbds } from "../canon/api";
import { listWorlds } from "../worlds/api";
import type { Scene, SceneReadiness } from "./types";

interface SceneReadinessPanelProps {
  projectRootPath: string;
  sceneId: string;
  refreshKey?: number;
}

function deriveReadiness(
  scene: Scene,
  worldHealth: string | null,
  charBroken: boolean,
  propBroken: boolean,
  hasTbdBlocker: boolean,
  hasUpgradeWarning: boolean,
  hasHistorical: boolean,
): SceneReadiness {
  const blockers: SceneReadiness["blockers"] = [];
  const warnings: SceneReadiness["warnings"] = [];

  if (!scene.title.trim()) {
    blockers.push({ kind: "title_missing", message: "Title is required" });
  }
  if (!scene.summary.trim()) {
    blockers.push({ kind: "summary_missing", message: "Summary is required" });
  }
  if (!scene.worldId || !scene.worldAssetVersionId) {
    blockers.push({ kind: "world_reference_missing", message: "World reference is required" });
  } else if (worldHealth === "broken") {
    blockers.push({ kind: "world_reference_broken", message: "World reference is broken", context: scene.worldAssetVersionId });
  }
  if (charBroken) {
    blockers.push({ kind: "character_reference_broken", message: "Character reference is broken" });
  }
  if (propBroken) {
    blockers.push({ kind: "prop_reference_broken", message: "Prop reference is broken" });
  }
  if (hasTbdBlocker) {
    blockers.push({ kind: "tbd_decision_required", message: "TBD decision required for relevant protected unknowns" });
  }

  if (hasUpgradeWarning) {
    warnings.push({ kind: "upgrade_available", message: "Upgrade available for one or more references", context: "upgrade_available" });
  }
  if (hasHistorical) {
    warnings.push({ kind: "historical_reference", message: "Historical reference — no current canonical exists" });
  }

  return {
    readyForKeyframe: blockers.length === 0,
    blockers,
    warnings,
  };
}

export function SceneReadinessPanel({ projectRootPath, sceneId, refreshKey = 0 }: SceneReadinessPanelProps) {
  const [readiness, setReadiness] = useState<SceneReadiness | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    async function load() {
      try {
        const scene = await getScene(projectRootPath, sceneId);
        const [resolved, allTbds, characters, worlds, bindings] = await Promise.all([
          resolveSceneReferences(projectRootPath, sceneId).catch(() => null),
          listCanonTbds(projectRootPath).catch(() => []),
          listSceneCharacters(projectRootPath, sceneId).catch(() => []),
          listWorlds(projectRootPath).catch(() => []),
          listSceneTbdBindings(projectRootPath, sceneId).catch(() => []),
        ]);

        // health detection
        const worldHealth = resolved?.world?.health ?? null;
        const charBroken = (resolved?.characters ?? []).some((c) => c.look.health === "broken" || c.sheet?.health === "broken");
        const propBroken = (resolved?.props ?? []).some((p) => p.reference.health === "broken");
        const hasUpgradeWarning =
          resolved?.world?.health === "upgrade_available" ||
          (resolved?.characters ?? []).some((c) => c.look.health === "upgrade_available" || c.sheet?.health === "upgrade_available") ||
          (resolved?.props ?? []).some((p) => p.reference.health === "upgrade_available");
        const hasHistorical =
          resolved?.world?.health === "historical" ||
          (resolved?.characters ?? []).some((c) => c.look.health === "historical" || c.sheet?.health === "historical") ||
          (resolved?.props ?? []).some((p) => p.reference.health === "historical");

        // TBD blocker: a relevant protected open TBD blocks keyframe
        // readiness until the scene carries an explicit persisted decision
        // for it (set from the TBD decisions panel on this page).
        const entityIds = new Set<string>();
        if (scene.worldId) {
          const world = worlds.find((w) => w.id === scene.worldId);
          if (world) entityIds.add(world.canonLocationEntityId);
        }
        for (const ch of characters) entityIds.add(ch.characterEntityId);
        const decidedTbdIds = new Set((bindings ?? []).map((binding) => binding.canonTbdId));
        const relevant = (allTbds ?? []).filter((tbd) => {
          if (!tbd.protected || tbd.status !== "open") return false;
          if (tbd.canonEntityId === null) return true;
          return entityIds.has(tbd.canonEntityId);
        });
        const undecidedRelevant = relevant.filter((tbd) => !decidedTbdIds.has(tbd.id));
        const hasTbdBlocker = undecidedRelevant.length > 0;

        const derived = deriveReadiness(
          scene,
          worldHealth,
          charBroken,
          propBroken,
          hasTbdBlocker,
          hasUpgradeWarning,
          hasHistorical,
        );

        if (!cancelled) setReadiness(derived);
      } catch (caught: unknown) {
        if (!cancelled) setError(describeError(caught));
      } finally {
        if (!cancelled) setLoading(false);
      }
    }
    void load();
    return () => {
      cancelled = true;
    };
  }, [projectRootPath, sceneId, refreshKey]);

  if (loading) return <p role="status">Loading readiness…</p>;
  if (error) return <p role="alert">{error}</p>;
  if (!readiness) return <p role="alert">Readiness not available</p>;

  return (
    <section
      aria-label="Scene readiness"
      style={{ padding: "var(--space-16)", background: "var(--c-panel-soft)", border: "1px solid var(--c-hairline)", borderRadius: "var(--radius-lg)" }}
    >
      <header style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline" }}>
        <h3 style={{ margin: 0, textTransform: "uppercase", fontSize: "var(--fs-md)", letterSpacing: "0.04em" }}>READINESS</h3>
        <span
          className={readiness.readyForKeyframe ? "canon-badge" : "canon-status canon-status--draft"}
          style={{ fontSize: "var(--fs-sm)", fontWeight: 600 }}
        >
          {readiness.readyForKeyframe ? "READY FOR KEYFRAME" : "NOT READY"}
        </span>
      </header>

      {readiness.blockers.length > 0 ? (
        <div style={{ marginTop: "var(--space-12)" }}>
          <h4 style={{ margin: 0, fontSize: "var(--fs-sm)", color: "var(--c-danger)" }}>Blockers</h4>
          <ul style={{ margin: "var(--space-4) 0 0", paddingLeft: "var(--space-16)" }}>
            {readiness.blockers.map((blocker, idx) => (
              <li key={idx} style={{ fontSize: "var(--fs-md)", color: "var(--c-danger)" }}>
                {blocker.kind}: {blocker.message} {blocker.context ? `(${blocker.context})` : ""}
              </li>
            ))}
          </ul>
        </div>
      ) : (
        <p style={{ marginTop: "var(--space-12)", fontSize: "var(--fs-md)", color: "var(--c-success)" }}>No blockers — Scene can generate a keyframe.</p>
      )}

      {readiness.warnings.length > 0 ? (
        <div style={{ marginTop: "var(--space-12)" }}>
          <h4 style={{ margin: 0, fontSize: "var(--fs-sm)", color: "var(--c-warning)" }}>Warnings</h4>
          <ul style={{ margin: "var(--space-4) 0 0", paddingLeft: "var(--space-16)" }}>
            {readiness.warnings.map((warning, idx) => (
              <li key={idx} style={{ fontSize: "var(--fs-md)", color: "var(--c-warning)" }}>
                {warning.kind}: {warning.message}
              </li>
            ))}
          </ul>
        </div>
      ) : null}

      <p style={{ marginTop: "var(--space-8)", fontSize: "var(--fs-sm)", color: "var(--c-muted)" }}>
        Upgrade available is a warning, not a blocker. Historical pins remain executable.
      </p>
    </section>
  );
}
