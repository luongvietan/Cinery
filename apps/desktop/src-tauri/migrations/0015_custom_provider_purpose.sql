ALTER TABLE custom_provider_definitions
ADD COLUMN purpose TEXT NOT NULL DEFAULT 'legacy'
CHECK (purpose IN ('legacy', 'llm', 'image', 'video'));
