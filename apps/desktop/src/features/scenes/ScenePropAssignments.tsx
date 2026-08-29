import { useEffect, useRef, useState } from "react";
import { describeError } from "../../lib/errors";
import { formatVersionNumber } from "@cinematic/domain";
import { listAssets, getAssetWithVersions } from "../assets/api";
import type { AssetSummary } from "@cinematic/domain";
import {
  addSceneProp,
  removeSceneProp,
  listSceneProps,
  resolveSceneReferences,
  upgradeScenePropReference,
} from "./api";
import type { ScenePropAssignment, ResolvedPropReference } from "./types";

interface ScenePropAssignmentsProps {
  projectRootPath: string;
  sceneId: string;
  onChanged?: () => void;
}

export function ScenePropAssignments({
  projectRootPath,
  sceneId,
  onChanged,
}: ScenePropAssignmentsProps) {
  const [assignments, setAssignments] = useState<ScenePropAssignment[]>([]);
  const [resolved, setResolved] = useState<ResolvedPropReference[]>([]);
  const [propAssets, setPropAssets] = useState<AssetSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [isDialogOpen, setIsDialogOpen] = useState(false);
  const [selectedVersionId, setSelectedVersionId] = useState("");
  const [label, setLabel] = useState("");
  const [notes, setNotes] = useState("");
  const [availableVersions, setAvailableVersions] = useState<{ id: string; assetLabel: string; versionNumber: number }[]>([]);
  const [confirmUpgrade, setConfirmUpgrade] = useState<{
    assignmentId: string;
    pinned: string;
    canonical: string | null;
    versionNumber: number;
  } | null>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const dialogRef = useRef<HTMLDivElement>(null);
  const confirmRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (isDialogOpen) {
      const id = requestAnimationFrame(() => dialogRef.current?.querySelector<HTMLElement>("select, input, button")?.focus());
      return () => cancelAnimationFrame(id);
    } else {
      requestAnimationFrame(() => triggerRef.current?.focus());
    }
  }, [isDialogOpen]);

  useEffect(() => {
    if (confirmUpgrade) {
      const id = requestAnimationFrame(() => confirmRef.current?.focus());
      return () => cancelAnimationFrame(id);
    }
  }, [confirmUpgrade]);

  useEffect(() => {
    if (!isDialogOpen) return;
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") setIsDialogOpen(false);
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [isDialogOpen]);

  useEffect(() => {
    if (!confirmUpgrade) return;
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") setConfirmUpgrade(null);
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [confirmUpgrade]);

  async function refresh() {
    setLoading(true);
    setError(null);
    try {
      const [assetList, assigns, resolvedData] = await Promise.all([
        listAssets(projectRootPath).catch(() => [] as AssetSummary[]),
        listSceneProps(projectRootPath, sceneId).catch(() => [] as ScenePropAssignment[]),
        resolveSceneReferences(projectRootPath, sceneId).catch(() => null),
      ]);
      const propsOnly = (assetList ?? []).filter((a) => a.type === "prop_plate");
      setPropAssets(propsOnly);
      setAssignments(assigns ?? []);
      setResolved(resolvedData?.props ?? []);

      // Load available canonical versions for prop plates
      const versions: typeof availableVersions = [];
      for (const asset of propsOnly) {
        if (!asset.canonicalVersionId) continue;
        try {
          const data = await getAssetWithVersions(projectRootPath, asset.id);
          const canonical = data.versions.find((v) => v.id === data.asset.canonicalVersionId);
          if (canonical) {
            // only include if not already assigned
            const already = (assigns ?? []).some((a) => a.propAssetVersionId === canonical.id);
            if (!already) versions.push({ id: canonical.id, assetLabel: asset.label, versionNumber: canonical.versionNumber });
          }
        } catch {}
      }
      setAvailableVersions(versions);
      if (versions.length > 0 && !selectedVersionId) setSelectedVersionId(versions[0].id);
    } catch (caught: unknown) {
      setError(describeError(caught));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectRootPath, sceneId]);

  async function handleAdd() {
    if (!selectedVersionId) {
      setActionError("Select a prop version");
      return;
    }
    setActionError(null);
    try {
      await addSceneProp(projectRootPath, sceneId, selectedVersionId, label || null, notes || null);
      setIsDialogOpen(false);
      setLabel("");
      setNotes("");
      setSelectedVersionId("");
      await refresh();
      onChanged?.();
    } catch (caught: unknown) {
      setActionError(describeError(caught));
    }
  }

  async function handleRemove(propAssetVersionId: string) {
    setActionError(null);
    try {
      await removeSceneProp(projectRootPath, sceneId, propAssetVersionId);
      await refresh();
      onChanged?.();
    } catch (caught: unknown) {
      setActionError(describeError(caught));
    }
  }

  async function handleUpgrade(assignmentId: string) {
    if (!confirmUpgrade) return;
    setActionError(null);
    try {
      await upgradeScenePropReference(projectRootPath, sceneId, assignmentId);
      setConfirmUpgrade(null);
      await refresh();
      onChanged?.();
    } catch (caught: unknown) {
      setActionError(describeError(caught));
    }
  }

  if (loading) return <p role="status">Loading props…</p>;
  if (error) return <p role="alert">{error}</p>;

  return (
    <section
      aria-label="Prop assignments"
      style={{ padding: "16px", background: "var(--surface-canvas)", border: "1px solid var(--color-hairline)", borderRadius: "10px" }}
    >
      <header style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
        <h3 style={{ margin: 0, textTransform: "uppercase", fontSize: "13px", letterSpacing: "0.04em" }}>PROPS</h3>
        <button type="button" ref={triggerRef} onClick={() => setIsDialogOpen(true)}>
          Add Prop
        </button>
      </header>

      {actionError ? <p role="alert" style={{ marginTop: "8px" }}>{actionError}</p> : null}

      {assignments.length === 0 ? (
        <p style={{ marginTop: "12px" }}>No props assigned. Pin exact canonical prop_plate versions.</p>
      ) : (
        <ul style={{ listStyle: "none", padding: 0, margin: "12px 0 0", display: "flex", flexDirection: "column", gap: "12px" }}>
          {assignments.map((assignment) => {
            const resolvedEntry = resolved.find((r) => r.assignmentId === assignment.id);
            const versionNumber = resolvedEntry?.reference.versionNumber ?? 0;
            const health = resolvedEntry?.reference.health ?? "unknown";
            const pinned = assignment.propAssetVersionId;
            const assetLabel = propAssets.find((a) => a.canonicalVersionId === pinned)?.label ?? resolvedEntry?.reference.assetId ?? pinned;
            return (
              <li
                key={assignment.id}
                style={{ padding: "12px", background: "var(--surface-card)", border: "1px solid var(--color-hairline)", borderRadius: "6px" }}
              >
                <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline" }}>
                  <strong>{assignment.label ?? assetLabel}</strong>
                  <button type="button" className="canon-secondary-button" onClick={() => void handleRemove(assignment.propAssetVersionId)}>
                    Remove
                  </button>
                </div>
                <div style={{ marginTop: "8px", display: "flex", flexDirection: "column", gap: "6px", fontSize: "13px" }}>
                  <div style={{ fontFamily: "ui-monospace, monospace", wordBreak: "break-all" }}>
                    {pinned} · {resolvedEntry ? formatVersionNumber(versionNumber) : ""} {assignment.label ? `· ${assignment.label}` : ""}
                  </div>
                  <div style={{ display: "flex", gap: "8px", alignItems: "center", flexWrap: "wrap" }}>
                    <span>Exact pinned version:</span>
                    <span style={{ fontFamily: "ui-monospace, monospace" }}>{pinned}</span>
                    {resolvedEntry ? <span className="asset-version-badge" style={{ fontSize: "11px" }}>{health.toUpperCase()}</span> : null}
                  </div>
                  {resolvedEntry ? (
                    <div style={{ fontFamily: "ui-monospace, monospace", fontSize: "12px" }}>
                      PINNED V{String(versionNumber).padStart(2, "0")} ({formatVersionNumber(versionNumber)}) · CURRENT CANONICAL {resolvedEntry.reference.currentCanonicalVersionId ?? "none"} · {health.toUpperCase()}
                    </div>
                  ) : null}
                  {assignment.notes ? <div>Notes: {assignment.notes}</div> : null}
                  {resolvedEntry?.reference.health === "upgrade_available" ? (
                    <button
                      type="button"
                      onClick={() =>
                        setConfirmUpgrade({
                          assignmentId: assignment.id,
                          pinned: resolvedEntry.reference.pinnedVersionId,
                          canonical: resolvedEntry.reference.currentCanonicalVersionId,
                          versionNumber,
                        })
                      }
                    >
                      Upgrade Scene to V{String(versionNumber + 1).padStart(2, "0")}
                    </button>
                  ) : null}
                </div>
              </li>
            );
          })}
        </ul>
      )}

      {isDialogOpen ? (
        <div className="canon-dialog-backdrop" role="presentation" onClick={() => setIsDialogOpen(false)}>
          <div
            ref={dialogRef}
            role="dialog"
            aria-modal="true"
            aria-labelledby="add-prop-title"
            className="canon-dialog"
            onClick={(event) => event.stopPropagation()}
          >
            <header>
              <h2 id="add-prop-title">Add Prop</h2>
              <button type="button" className="canon-secondary-button" onClick={() => setIsDialogOpen(false)} aria-label="Close">
                ✕
              </button>
            </header>
            <p>Select an exact canonical prop_plate version. Only canonical prop versions can be pinned.</p>
            {actionError ? <p role="alert">{actionError}</p> : null}
            <div className="canon-field-grid" style={{ marginTop: "12px" }}>
              <label htmlFor="prop-select">
                Prop version
                <select
                  id="prop-select"
                  value={selectedVersionId}
                  onChange={(event) => setSelectedVersionId(event.target.value)}
                  aria-label="Prop version"
                >
                  <option value="">Select a prop_plate canonical version</option>
                  {availableVersions.map((v) => (
                    <option key={v.id} value={v.id}>
                      {v.assetLabel} — {v.id} · {formatVersionNumber(v.versionNumber)}
                    </option>
                  ))}
                </select>
              </label>
              <label htmlFor="prop-label">
                Label (optional)
                <input id="prop-label" value={label} onChange={(event) => setLabel(event.target.value)} placeholder="e.g. Hero prop" />
              </label>
              <label htmlFor="prop-notes">
                Notes (optional)
                <input id="prop-notes" value={notes} onChange={(event) => setNotes(event.target.value)} placeholder="Placement notes" />
              </label>
            </div>
            {availableVersions.length === 0 ? (
              <p style={{ marginTop: "8px", color: "var(--color-mid-gray)", fontSize: "12px" }}>No canonical prop_plate versions available. Import and promote a prop_plate asset first.</p>
            ) : null}
            <div style={{ display: "flex", gap: "8px", marginTop: "16px" }}>
              <button type="button" onClick={() => void handleAdd()} disabled={!selectedVersionId}>
                Add Prop
              </button>
              <button type="button" className="canon-secondary-button" onClick={() => setIsDialogOpen(false)}>
                Cancel
              </button>
            </div>
          </div>
        </div>
      ) : null}

      {confirmUpgrade ? (
        <div className="canon-dialog-backdrop" role="presentation" onClick={() => setConfirmUpgrade(null)}>
          <div role="dialog" aria-modal="true" aria-labelledby="upgrade-prop-title" className="canon-dialog" onClick={(event) => event.stopPropagation()}>
            <header>
              <h2 id="upgrade-prop-title">Confirm upgrade</h2>
              <button type="button" className="canon-secondary-button" onClick={() => setConfirmUpgrade(null)} aria-label="Close">
                ✕
              </button>
            </header>
            <div style={{ display: "flex", flexDirection: "column", gap: "12px", marginTop: "12px" }}>
              <p>
                Upgrade prop reference for scene {sceneId}?
              </p>
              <div style={{ display: "grid", gap: "8px", padding: "12px", background: "var(--surface-canvas)", border: "1px solid var(--color-hairline)", borderRadius: "6px", fontFamily: "ui-monospace, monospace", fontSize: "12px" }}>
                <div>Pinned: {confirmUpgrade.pinned} (V{String(confirmUpgrade.versionNumber).padStart(2, "0")} / {formatVersionNumber(confirmUpgrade.versionNumber)})</div>
                <div>Current canonical: {confirmUpgrade.canonical ?? "none"}</div>
                <div>Old version: {confirmUpgrade.pinned}</div>
                <div>New version: {confirmUpgrade.canonical}</div>
                <div>Scene: {sceneId}</div>
              </div>
              <div style={{ display: "flex", gap: "8px" }}>
                <button type="button" ref={confirmRef} onClick={() => void handleUpgrade(confirmUpgrade.assignmentId)}>
                  Upgrade Scene to V{String(confirmUpgrade.versionNumber + 1).padStart(2, "0")}
                </button>
                <button type="button" className="canon-secondary-button" onClick={() => setConfirmUpgrade(null)}>
                  Cancel
                </button>
              </div>
              <p style={{ fontSize: "12px", fontWeight: 600 }}>Upgrade Scene to V{String(confirmUpgrade.versionNumber + 1).padStart(2, "0")}</p>
            </div>
          </div>
        </div>
      ) : null}
    </section>
  );
}
