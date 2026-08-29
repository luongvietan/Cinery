-- Relax artifact_promotions uniqueness (migration 0017 follow-up).
--
-- `asset_versions` dedups by sha256: importing content that already exists
-- as a version returns the existing immutable version. Two generation runs
-- can therefore legitimately produce two distinct artifacts whose promoted
-- content resolves to ONE version (deterministic mock/dry-run output is the
-- common case). `UNIQUE(asset_version_id)` — and `UNIQUE(artifact_id)`'s
-- implied one-row-per-artifact — made that idempotent case fail with a
-- constraint error instead of recording both promotions.
--
-- Recreate the table without the column-level UNIQUE constraints; lookup
-- performance is preserved with indexes. Migration is content-preserving.

CREATE TABLE artifact_promotions_migrated (
    id TEXT PRIMARY KEY,
    artifact_id TEXT NOT NULL,
    asset_id TEXT NOT NULL,
    asset_version_id TEXT NOT NULL,
    set_canonical INTEGER NOT NULL CHECK (set_canonical IN (0, 1)),
    created_at TEXT NOT NULL,
    FOREIGN KEY (artifact_id) REFERENCES generated_artifacts(id),
    FOREIGN KEY (asset_id) REFERENCES assets(id),
    FOREIGN KEY (asset_version_id) REFERENCES asset_versions(id)
);

INSERT INTO artifact_promotions_migrated (id, artifact_id, asset_id, asset_version_id, set_canonical, created_at)
SELECT id, artifact_id, asset_id, asset_version_id, set_canonical, created_at
FROM artifact_promotions;

DROP TABLE artifact_promotions;
ALTER TABLE artifact_promotions_migrated RENAME TO artifact_promotions;

CREATE INDEX idx_artifact_promotions_artifact ON artifact_promotions(artifact_id);
CREATE INDEX idx_artifact_promotions_version ON artifact_promotions(asset_version_id);
