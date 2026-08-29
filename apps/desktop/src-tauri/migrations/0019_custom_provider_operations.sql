-- Provider platform refactor: custom providers gain a declarative runtime
-- configuration (auth mode, per-operation endpoint definitions, request/
-- response mappings, async job lifecycles). Existing rows keep NULL here and
-- are synthesized from their legacy `purpose` on read; the next save
-- persists the compiled configuration. No data is destroyed.
ALTER TABLE custom_provider_definitions
ADD COLUMN definition_json TEXT;
