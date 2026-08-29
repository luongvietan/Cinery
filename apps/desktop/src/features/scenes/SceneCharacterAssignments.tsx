import { useEffect, useRef, useState } from "react";
import { describeError } from "../../lib/errors";
import { formatVersionNumber } from "@cinematic/domain";
import { listCanonEntities } from "../canon/api";
import type { CanonEntity } from "@cinematic/domain";
import { getAssetWithVersions, listAssets } from "../assets/api";
import type { AssetSummary } from "@cinematic/domain";
import {
  addSceneCharacter,
  removeSceneCharacter,
  listSceneCharacters,
  resolveSceneReferences,
  upgradeSceneCharacterLookReference,
  upgradeSceneCharacterSheetReference,
} from "./api";
import type { SceneCharacterAssignment, ResolvedCharacterReference } from "./types";

interface SceneCharacterAssignmentsProps {
  projectRootPath: string;
  sceneId: string;
  onChanged?: () => void;
}

export function SceneCharacterAssignments({
  projectRootPath,
  sceneId,
  onChanged,
}: SceneCharacterAssignmentsProps) {
  const [assignments, setAssignments] = useState<SceneCharacterAssignment[]>([]);
  const [resolved, setResolved] = useState<ResolvedCharacterReference[]>([]);
  const [characters, setCharacters] = useState<CanonEntity[]>([]);
  const [assets, setAssets] = useState<AssetSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [isDialogOpen, setIsDialogOpen] = useState(false);
  const [selectedCharacterId, setSelectedCharacterId] = useState("");
  const [selectedLookVersionId, setSelectedLookVersionId] = useState("");
  const [selectedSheetVersionId, setSelectedSheetVersionId] = useState("");
  const [notes, setNotes] = useState("");
  const [lookVersions, setLookVersions] = useState<{ id: string; versionNumber: number; assetLabel: string }[]>([]);
  const [sheetVersions, setSheetVersions] = useState<{ id: string; versionNumber: number; assetLabel: string }[]>([]);
  const [upgradingId, setUpgradingId] = useState<string | null>(null);
  const [confirmUpgrade, setConfirmUpgrade] = useState<{
    assignmentId: string;
    kind: "look" | "sheet";
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
    } else if (confirmUpgrade === null) {
      // return focus? not needed
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
      const [chars, assetList, assigns, resolvedData] = await Promise.all([
        listCanonEntities(projectRootPath, "character").catch(() => [] as CanonEntity[]),
        listAssets(projectRootPath).catch(() => [] as AssetSummary[]),
        listSceneCharacters(projectRootPath, sceneId).catch(() => [] as SceneCharacterAssignment[]),
        resolveSceneReferences(projectRootPath, sceneId).catch(() => null),
      ]);
      setCharacters(chars ?? []);
      setAssets(assetList ?? []);
      setAssignments(assigns ?? []);
      setResolved(resolvedData?.characters ?? []);
      if (chars && chars.length > 0 && !selectedCharacterId) {
        // auto-select first not already assigned
        const assignedIds = new Set((assigns ?? []).map((a) => a.characterEntityId));
        const firstAvailable = chars.find((c) => !assignedIds.has(c.id));
        if (firstAvailable) setSelectedCharacterId(firstAvailable.id);
      }
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

  // When character changes, load look/sheet versions for that character's assets
  useEffect(() => {
    if (!selectedCharacterId) {
      setLookVersions([]);
      setSheetVersions([]);
      setSelectedLookVersionId("");
      setSelectedSheetVersionId("");
      return;
    }
    // Find assets owned by character
    const ownedAssets = assets.filter((a) => a.ownerEntityId === selectedCharacterId);
    // For each asset, fetch versions to find canonical ones
    let cancelled = false;
    async function loadVersions() {
      const look: typeof lookVersions = [];
      const sheet: typeof sheetVersions = [];
      for (const asset of ownedAssets) {
        try {
          const data = await getAssetWithVersions(projectRootPath, asset.id);
          const canonical = data.versions.find((v) => v.id === data.asset.canonicalVersionId);
          if (!canonical) continue;
          // Determine if asset is sheet vs look: type character_sheet or outfit
          if (asset.type === "character_sheet" || asset.type === "outfit") {
            if (asset.type === "character_sheet") {
              sheet.push({ id: canonical.id, versionNumber: canonical.versionNumber, assetLabel: asset.label });
            } else {
              look.push({ id: canonical.id, versionNumber: canonical.versionNumber, assetLabel: asset.label });
            }
          } else if (asset.type === "face_lock" || asset.type === "image") {
            look.push({ id: canonical.id, versionNumber: canonical.versionNumber, assetLabel: asset.label });
          }
        } catch {
          // ignore
        }
      }
      // Fallback: if no outfit found, try any asset with canonical that is not world_plate/prop_plate
      if (look.length === 0) {
        for (const asset of ownedAssets) {
          if (asset.type === "world_plate" || asset.type === "prop_plate" || asset.type === "shot_keyframe") continue;
          if (asset.type === "character_sheet") continue;
          try {
            const data = await getAssetWithVersions(projectRootPath, asset.id);
            const canonical = data.versions.find((v) => v.id === data.asset.canonicalVersionId);
            if (canonical) look.push({ id: canonical.id, versionNumber: canonical.versionNumber, assetLabel: asset.label });
          } catch {}
        }
      }
      if (!cancelled) {
        setLookVersions(look);
        setSheetVersions(sheet);
        if (look.length > 0 && !selectedLookVersionId) setSelectedLookVersionId(look[0].id);
        else if (look.length === 0) setSelectedLookVersionId("");
        if (sheet.length > 0) {
          // auto-select first sheet? Leave empty for optional
        } else {
          setSelectedSheetVersionId("");
        }
      }
    }
    void loadVersions();
    return () => {
      cancelled = true;
    };
  }, [selectedCharacterId, assets, projectRootPath, selectedLookVersionId]);

  async function handleAdd() {
    if (!selectedCharacterId || !selectedLookVersionId) {
      setActionError("Select a character and a look version");
      return;
    }
    setActionError(null);
    try {
      await addSceneCharacter(
        projectRootPath,
        sceneId,
        selectedCharacterId,
        selectedLookVersionId,
        selectedSheetVersionId || null,
        notes || null,
      );
      setIsDialogOpen(false);
      setSelectedSheetVersionId("");
      setNotes("");
      await refresh();
      onChanged?.();
    } catch (caught: unknown) {
      setActionError(describeError(caught));
    }
  }

  async function handleRemove(characterEntityId: string) {
    setActionError(null);
    try {
      await removeSceneCharacter(projectRootPath, sceneId, characterEntityId);
      await refresh();
      onChanged?.();
    } catch (caught: unknown) {
      setActionError(describeError(caught));
    }
  }

  async function handleUpgradeLook(assignmentId: string) {
    if (!confirmUpgrade) return;
    setUpgradingId(assignmentId);
    setActionError(null);
    try {
      await upgradeSceneCharacterLookReference(projectRootPath, sceneId, assignmentId);
      setConfirmUpgrade(null);
      await refresh();
      onChanged?.();
    } catch (caught: unknown) {
      setActionError(describeError(caught));
    } finally {
      setUpgradingId(null);
    }
  }

  async function handleUpgradeSheet(assignmentId: string) {
    if (!confirmUpgrade) return;
    setUpgradingId(assignmentId);
    setActionError(null);
    try {
      await upgradeSceneCharacterSheetReference(projectRootPath, sceneId, assignmentId);
      setConfirmUpgrade(null);
      await refresh();
      onChanged?.();
    } catch (caught: unknown) {
      setActionError(describeError(caught));
    } finally {
      setUpgradingId(null);
    }
  }

  if (loading) return <p role="status">Loading characters…</p>;
  if (error) return <p role="alert">{error}</p>;

  return (
    <section
      aria-label="Character assignments"
      style={{ padding: "var(--space-16)", background: "var(--c-panel)", border: "1px solid var(--c-hairline)", borderRadius: "var(--radius-lg)" }}
    >
      <header style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
        <h3 style={{ margin: 0, textTransform: "uppercase", fontSize: "var(--fs-md)", letterSpacing: "0.04em" }}>CHARACTERS</h3>
        <button type="button" ref={triggerRef} onClick={() => setIsDialogOpen(true)}>
          Add Character
        </button>
      </header>

      {actionError ? <p role="alert" style={{ marginTop: "var(--space-8)" }}>{actionError}</p> : null}

      {assignments.length === 0 ? (
        <p style={{ marginTop: "var(--space-12)" }}>No characters assigned. Pin exact Look versions.</p>
      ) : (
        <ul style={{ listStyle: "none", padding: 0, margin: "var(--space-12) 0 0", display: "flex", flexDirection: "column", gap: "var(--space-12)" }}>
          {assignments.map((assignment) => {
            const character = characters.find((c) => c.id === assignment.characterEntityId);
            const characterName = character?.name ?? assignment.characterEntityId;
            const resolvedEntry = resolved.find((r) => r.assignmentId === assignment.id);
            const lookHealth = resolvedEntry?.look.health ?? "unknown";
            const sheetHealth = resolvedEntry?.sheet?.health ?? null;
            const lookPinned = assignment.lookAssetVersionId;
            const sheetPinned = assignment.sheetAssetVersionId;

            // Find asset labels for look/sheet versions if possible via assets?
            // We don't have direct mapping, but we can show version ids
            const lookLabel = resolvedEntry?.look.assetId
              ? assets.find((a) => a.id === resolvedEntry.look.assetId)?.label ?? resolvedEntry.look.assetId
              : lookPinned;
            const sheetLabel = resolvedEntry?.sheet
              ? assets.find((a) => a.id === resolvedEntry.sheet!.assetId)?.label ?? resolvedEntry.sheet!.assetId
              : sheetPinned;

            return (
              <li
                key={assignment.id}
                style={{ padding: "var(--space-12)", background: "var(--c-panel-soft)", border: "1px solid var(--c-hairline)", borderRadius: "var(--radius-md)" }}
              >
                <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline" }}>
                  <strong>{characterName}</strong>
                  <button
                    type="button"
                    className="canon-secondary-button"
                    onClick={() => void handleRemove(assignment.characterEntityId)}
                  >
                    Remove
                  </button>
                </div>
                <div style={{ marginTop: "var(--space-8)", display: "flex", flexDirection: "column", gap: "var(--space-8)", fontSize: "var(--fs-md)" }}>
                  <div>
                    <span style={{ fontWeight: 500 }}>Character:</span> {characterName} ({assignment.characterEntityId})
                  </div>
                  <div style={{ display: "flex", gap: "var(--space-8)", alignItems: "center", flexWrap: "wrap" }}>
                    <span style={{ fontWeight: 500 }}>Look:</span>
                    <span style={{ fontFamily: "var(--font-mono)", wordBreak: "break-all" }}>
                      {lookLabel} — {lookPinned} {resolvedEntry ? `· ${formatVersionNumber(resolvedEntry.look.versionNumber)}` : ""}
                    </span>
                    {resolvedEntry ? (
                      <span className="asset-version-badge" style={{ fontSize: "var(--fs-xs)" }}>
                        {lookHealth.toUpperCase()}
                      </span>
                    ) : null}
                  </div>
                  {resolvedEntry ? (
                    <div style={{ fontFamily: "var(--font-mono)", fontSize: "var(--fs-sm)" }}>
                      PINNED V{String(resolvedEntry.look.versionNumber).padStart(2, "0")} ({formatVersionNumber(resolvedEntry.look.versionNumber)}) · CURRENT CANONICAL{" "}
                      {resolvedEntry.look.currentCanonicalVersionId ?? "none"} · {lookHealth.toUpperCase()}
                    </div>
                  ) : null}
                  {resolvedEntry?.look.health === "upgrade_available" ? (
                    <button
                      type="button"
                      onClick={() =>
                        setConfirmUpgrade({
                          assignmentId: assignment.id,
                          kind: "look",
                          pinned: resolvedEntry.look.pinnedVersionId,
                          canonical: resolvedEntry.look.currentCanonicalVersionId,
                          versionNumber: resolvedEntry.look.versionNumber,
                        })
                      }
                      disabled={upgradingId === assignment.id}
                    >
                      Upgrade Scene to {resolvedEntry.look.currentCanonicalVersionId ? `V${String(resolvedEntry.look.versionNumber + 1).padStart(2, "0")}` : "V02"}
                    </button>
                  ) : null}
                  {sheetPinned ? (
                    <div style={{ display: "flex", gap: "var(--space-8)", alignItems: "center", flexWrap: "wrap" }}>
                      <span style={{ fontWeight: 500 }}>Sheet:</span>
                      <span style={{ fontFamily: "var(--font-mono)", wordBreak: "break-all" }}>
                        {sheetLabel} — {sheetPinned} {resolvedEntry?.sheet ? `· ${formatVersionNumber(resolvedEntry.sheet.versionNumber)}` : ""}
                      </span>
                      {sheetHealth ? (
                        <span className="asset-version-badge" style={{ fontSize: "var(--fs-xs)" }}>
                          {sheetHealth.toUpperCase()}
                        </span>
                      ) : null}
                    </div>
                  ) : (
                    <div style={{ fontSize: "var(--fs-sm)", color: "var(--c-muted)" }}>No Sheet (optional)</div>
                  )}
                  {resolvedEntry?.sheet ? (
                    <div style={{ fontFamily: "var(--font-mono)", fontSize: "var(--fs-sm)" }}>
                      PINNED V{String(resolvedEntry.sheet.versionNumber).padStart(2, "0")} ({formatVersionNumber(resolvedEntry.sheet.versionNumber)}) · CURRENT CANONICAL{" "}
                      {resolvedEntry.sheet.currentCanonicalVersionId ?? "none"} · {sheetHealth?.toUpperCase()}
                    </div>
                  ) : null}
                  {resolvedEntry?.sheet && resolvedEntry.sheet.health === "upgrade_available" ? (
                    <button
                      type="button"
                      onClick={() =>
                        setConfirmUpgrade({
                          assignmentId: assignment.id,
                          kind: "sheet",
                          pinned: resolvedEntry.sheet!.pinnedVersionId,
                          canonical: resolvedEntry.sheet!.currentCanonicalVersionId,
                          versionNumber: resolvedEntry.sheet!.versionNumber,
                        })
                      }
                    >
                      Upgrade Scene to {resolvedEntry.sheet.currentCanonicalVersionId ? `V${String(resolvedEntry.sheet.versionNumber + 1).padStart(2, "0")}` : "V02"}
                    </button>
                  ) : null}
                  {assignment.notes ? (
                    <div>
                      <span style={{ fontWeight: 500 }}>Notes:</span> {assignment.notes}
                    </div>
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
            aria-labelledby="add-character-title"
            className="canon-dialog"
            onClick={(event) => event.stopPropagation()}
          >
            <header>
              <h2 id="add-character-title">Add Character</h2>
              <button type="button" className="canon-secondary-button" onClick={() => setIsDialogOpen(false)} aria-label="Close">
                ✕
              </button>
            </header>
            <p>Pin an exact canonical Look version for the Character. Optional Sheet must also be canonical and owned by the same Character.</p>
            {actionError ? <p role="alert">{actionError}</p> : null}
            <div className="canon-field-grid" style={{ marginTop: "var(--space-12)" }}>
              <label htmlFor="char-select">
                Character
                <select
                  id="char-select"
                  value={selectedCharacterId}
                  onChange={(event) => setSelectedCharacterId(event.target.value)}
                >
                  <option value="">Select a Character</option>
                  {characters
                    .filter((c) => !assignments.some((a) => a.characterEntityId === c.id))
                    .map((c) => (
                      <option key={c.id} value={c.id}>
                        {c.name}
                      </option>
                    ))}
                </select>
              </label>
              <label htmlFor="look-select">
                Look alias/version
                <select
                  id="look-select"
                  value={selectedLookVersionId}
                  onChange={(event) => setSelectedLookVersionId(event.target.value)}
                >
                  <option value="">Select a Look version (canonical only)</option>
                  {lookVersions.map((lv) => (
                    <option key={lv.id} value={lv.id}>
                      {lv.assetLabel} — {lv.id} · {formatVersionNumber(lv.versionNumber)}
                    </option>
                  ))}
                </select>
              </label>
              <label htmlFor="sheet-select">
                Sheet (optional)
                <select
                  id="sheet-select"
                  value={selectedSheetVersionId}
                  onChange={(event) => setSelectedSheetVersionId(event.target.value)}
                >
                  <option value="">No Sheet</option>
                  {sheetVersions.map((sv) => (
                    <option key={sv.id} value={sv.id}>
                      {sv.assetLabel} — {sv.id} · {formatVersionNumber(sv.versionNumber)}
                    </option>
                  ))}
                </select>
              </label>
              <label htmlFor="char-notes">
                Notes
                <input id="char-notes" value={notes} onChange={(event) => setNotes(event.target.value)} placeholder="Optional notes" />
              </label>
            </div>
            <div style={{ display: "flex", gap: "var(--space-8)", marginTop: "var(--space-16)" }}>
              <button type="button" onClick={() => void handleAdd()} disabled={!selectedCharacterId || !selectedLookVersionId}>
                Add Character
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
          <div
            role="dialog"
            aria-modal="true"
            aria-labelledby="upgrade-char-title"
            className="canon-dialog"
            onClick={(event) => event.stopPropagation()}
          >
            <header>
              <h2 id="upgrade-char-title">Confirm upgrade</h2>
              <button type="button" className="canon-secondary-button" onClick={() => setConfirmUpgrade(null)} aria-label="Close">
                ✕
              </button>
            </header>
            <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-12)", marginTop: "var(--space-12)" }}>
              <p>
                Upgrade <strong>{confirmUpgrade.kind === "look" ? "Character Look" : "Character Sheet"}</strong> for scene {sceneId}?
              </p>
              <div style={{ display: "grid", gap: "var(--space-8)", padding: "var(--space-12)", background: "var(--c-panel)", border: "1px solid var(--c-hairline)", borderRadius: "var(--radius-md)", fontFamily: "var(--font-mono)", fontSize: "var(--fs-sm)" }}>
                <div>Pinned: {confirmUpgrade.pinned} (V{String(confirmUpgrade.versionNumber).padStart(2, "0")} / {formatVersionNumber(confirmUpgrade.versionNumber)})</div>
                <div>Current canonical: {confirmUpgrade.canonical ?? "none"}</div>
                <div>Old version: {confirmUpgrade.pinned}</div>
                <div>New version: {confirmUpgrade.canonical}</div>
                <div>Scene: {sceneId}</div>
              </div>
              <div style={{ display: "flex", gap: "var(--space-8)" }}>
                <button
                  type="button"
                  ref={confirmRef}
                  onClick={() => {
                    if (confirmUpgrade.kind === "look") void handleUpgradeLook(confirmUpgrade.assignmentId);
                    else void handleUpgradeSheet(confirmUpgrade.assignmentId);
                  }}
                  disabled={upgradingId !== null}
                >
                  Upgrade Scene to V{String(confirmUpgrade.versionNumber + 1).padStart(2, "0")}
                </button>
                <button type="button" className="canon-secondary-button" onClick={() => setConfirmUpgrade(null)}>
                  Cancel
                </button>
              </div>
              <p style={{ fontSize: "var(--fs-sm)", fontWeight: 600 }}>
                Upgrade Scene to V{String(confirmUpgrade.versionNumber + 1).padStart(2, "0")}
              </p>
            </div>
          </div>
        </div>
      ) : null}
    </section>
  );
}
