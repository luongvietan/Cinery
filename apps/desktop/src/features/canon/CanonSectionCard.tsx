import { useState, type ReactNode } from "react";
import type { CanonSection } from "@cinematic/domain";
import { describeError } from "../../lib/errors";

interface CanonSectionCardProps<T> {
  title: string;
  section: CanonSection<T> | null;
  draftValue: T;
  validate: (value: unknown) => T;
  renderEditor: (value: T, onChange: (value: T) => void) => ReactNode;
  renderReadOnly: (value: T) => ReactNode;
  onSave: (value: T) => Promise<void>;
  onLock: () => Promise<void>;
  onUnlock: () => Promise<void>;
  onHistory: () => void;
}

export function CanonSectionCard<T>({
  title,
  section,
  draftValue,
  validate,
  renderEditor,
  renderReadOnly,
  onSave,
  onLock,
  onUnlock,
  onHistory,
}: CanonSectionCardProps<T>) {
  const [value, setValue] = useState<T>(section?.value ?? draftValue);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const locked = section?.status === "locked";

  function validatedValue(): T | null {
    try {
      const parsed = validate(value);
      setError(null);
      return parsed;
    } catch (caught) {
      setError(describeError(caught));
      return null;
    }
  }

  function isValid(): boolean {
    try {
      validate(value);
      return true;
    } catch {
      return false;
    }
  }

  async function run(action: () => Promise<void>) {
    setBusy(true);
    setError(null);
    try {
      await action();
    } catch (caught) {
      setError(describeError(caught));
    } finally {
      setBusy(false);
    }
  }

  return (
    <article className="canon-section-card" aria-label={title}>
      <header className="canon-section-card__header">
        <div>
          <h3>{title}</h3>
          <span className={`canon-status canon-status--${section?.status ?? "draft"}`}>
            {section?.status?.toUpperCase() ?? "DRAFT"}
          </span>
          {section ? <span className="canon-revision">Revision {section.revision}</span> : null}
        </div>
        <button type="button" className="canon-secondary-button" onClick={onHistory}>
          History
        </button>
      </header>
      {locked ? (
        <div className="canon-readonly">{renderReadOnly(value)}</div>
      ) : (
        <div className="canon-editor">{renderEditor(value, setValue)}</div>
      )}
      {error ? <p role="alert">{error}</p> : null}
      <footer className="canon-section-card__actions">
        {!locked ? (
          <>
            <button
              type="button"
              onClick={() => {
                const parsed = validatedValue();
                if (parsed !== null) void run(() => onSave(parsed));
              }}
              disabled={busy}
            >
              Save Draft
            </button>
            <button
              type="button"
              className="canon-secondary-button"
              onClick={() => void run(onLock)}
              disabled={busy || !section || !isValid()}
            >
              Lock
            </button>
          </>
        ) : (
          <button type="button" onClick={() => void run(onUnlock)} disabled={busy}>
            Unlock
          </button>
        )}
      </footer>
    </article>
  );
}
