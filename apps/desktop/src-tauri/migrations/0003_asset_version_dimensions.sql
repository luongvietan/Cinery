ALTER TABLE asset_versions ADD COLUMN width INTEGER CHECK (width IS NULL OR width > 0);
ALTER TABLE asset_versions ADD COLUMN height INTEGER CHECK (height IS NULL OR height > 0);
