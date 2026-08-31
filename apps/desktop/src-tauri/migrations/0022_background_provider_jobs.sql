-- P10.1 durable background provider jobs: poll bookkeeping columns on the
-- durable provider_jobs row. SQLite stays the source of truth for execution
-- state; the runner derives progress and cadence from these persisted
-- values, never from in-memory task handles.
ALTER TABLE provider_jobs ADD COLUMN progress_percent INTEGER CHECK (progress_percent IS NULL OR (progress_percent >= 0 AND progress_percent <= 100));
ALTER TABLE provider_jobs ADD COLUMN last_polled_at TEXT;
-- The provider operation that created the job (e.g. `video.generate`).
-- Async declarative adapters need it to poll/fetch a job from a *rehydrated*
-- adapter instance (their in-memory job→operation map dies with the process);
-- `operation` on ProviderJobRef is persisted here so restart recovery works.
ALTER TABLE provider_jobs ADD COLUMN operation TEXT;
