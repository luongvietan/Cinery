import { useEffect, useRef, useState } from "react";
import type { CanonEntity } from "@cinematic/domain";
import { describeError } from "../../lib/errors";
import { listCanonEntities } from "../canon/api";
import { createWorld, listWorlds } from "./api";

interface CreateWorldButtonProps {
  projectRootPath: string;
  onCreated: (worldId: string) => void;
}

export function CreateWorldButton({
  projectRootPath,
  onCreated,
}: CreateWorldButtonProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [locations, setLocations] = useState<CanonEntity[]>([]);
  const [usedLocationIds, setUsedLocationIds] = useState<Set<string>>(
    () => new Set(),
  );
  const [selectedLocationId, setSelectedLocationId] = useState<string>("");
  const [loading, setLoading] = useState(false);
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const dialogRef = useRef<HTMLDivElement>(null);
  const selectRef = useRef<HTMLSelectElement>(null);

  useEffect(() => {
    if (!isOpen) return;
    let cancelled = false;
    setLoading(true);
    setError(null);
    Promise.all([
      listCanonEntities(projectRootPath, "location"),
      listWorlds(projectRootPath),
    ])
      .then(([nextLocations, worlds]) => {
        if (cancelled) return;
        setLocations(nextLocations ?? []);
        setUsedLocationIds(
          new Set(worlds.map((world) => world.canonLocationEntityId)),
        );
        // auto-select first available location
        const firstAvailable = (nextLocations ?? []).find(
          (loc) => !worlds.some((w) => w.canonLocationEntityId === loc.id),
        );
        setSelectedLocationId(firstAvailable?.id ?? "");
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
  }, [isOpen, projectRootPath]);

  useEffect(() => {
    if (isOpen) {
      // focus select after loading
      const id = requestAnimationFrame(() => selectRef.current?.focus());
      return () => cancelAnimationFrame(id);
    }
    // return focus to trigger
    if (!isOpen) {
      requestAnimationFrame(() => triggerRef.current?.focus());
    }
  }, [isOpen, loading]);

  // handle Escape
  useEffect(() => {
    if (!isOpen) return;
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        setIsOpen(false);
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [isOpen]);

  async function handleCreate() {
    if (!selectedLocationId) return;
    setCreating(true);
    setError(null);
    try {
      const created = await createWorld(projectRootPath, selectedLocationId);
      setIsOpen(false);
      setSelectedLocationId("");
      onCreated(created.id);
    } catch (caught: unknown) {
      setError(describeError(caught));
    } finally {
      setCreating(false);
    }
  }

  const availableCount = locations.filter(
    (loc) => !usedLocationIds.has(loc.id),
  ).length;
  const selectedIsUsed = selectedLocationId
    ? usedLocationIds.has(selectedLocationId)
    : false;

  return (
    <>
      <button
        type="button"
        ref={triggerRef}
        onClick={() => setIsOpen(true)}
      >
        New World
      </button>
      {isOpen ? (
        <div
          className="canon-dialog-backdrop"
          role="presentation"
          onClick={() => setIsOpen(false)}
        >
          <div
            ref={dialogRef}
            role="dialog"
            aria-modal="true"
            aria-labelledby="create-world-title"
            className="canon-dialog"
            onClick={(event) => event.stopPropagation()}
          >
            <header>
              <h2 id="create-world-title">Create World</h2>
              <button
                type="button"
                className="canon-secondary-button"
                onClick={() => setIsOpen(false)}
                aria-label="Close"
              >
                ✕
              </button>
            </header>
            <p>
              Select an existing Canon Location. A World is a production
              projection of that Location and owns a stable World Plate asset.
              Locations already used by a World are disabled.
            </p>
            {error ? <p role="alert">{error}</p> : null}
            {loading ? (
              <p role="status">Loading locations…</p>
            ) : locations.length === 0 ? (
              <p>No Locations yet. Create a Location in Canon first.</p>
            ) : availableCount === 0 ? (
              <p>All Locations already have Worlds.</p>
            ) : (
              <div className="canon-field-grid">
                <label htmlFor="world-location-select">
                  Canon Location
                </label>
                <select
                  id="world-location-select"
                  ref={selectRef}
                  value={selectedLocationId}
                  onChange={(event) =>
                    setSelectedLocationId(event.target.value)
                  }
                >
                  <option value="">Select a Location</option>
                  {locations.map((location) => {
                    const disabled = usedLocationIds.has(location.id);
                    return (
                      <option
                        key={location.id}
                        value={location.id}
                        disabled={disabled}
                      >
                        {location.name}
                        {disabled ? " — already has World" : ""}
                      </option>
                    );
                  })}
                </select>
              </div>
            )}
            <div
              style={{
                display: "flex",
                gap: "var(--space-8)",
                marginTop: "var(--space-16)",
              }}
            >
              <button
                type="button"
                onClick={() => void handleCreate()}
                disabled={
                  creating ||
                  loading ||
                  !selectedLocationId ||
                  selectedIsUsed
                }
              >
                {creating ? "Creating…" : "Create World"}
              </button>
              <button
                type="button"
                className="canon-secondary-button"
                onClick={() => setIsOpen(false)}
                disabled={creating}
              >
                Cancel
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </>
  );
}
