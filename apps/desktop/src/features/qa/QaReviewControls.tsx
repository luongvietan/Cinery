import { useState } from "react";
import type { QaReviewStatus } from "./types";

interface QaReviewControlsProps {
  label: string;
  disabled: boolean;
  initialNote: string | null;
  onReview: (status: QaReviewStatus, note: string | null) => Promise<void>;
}

export function QaReviewControls({
  label,
  disabled,
  initialNote,
  onReview,
}: QaReviewControlsProps) {
  const [note, setNote] = useState(initialNote ?? "");

  function submit(status: QaReviewStatus) {
    void onReview(status, note.trim() || null);
  }

  return (
    <div className="qa-review-controls">
      <label>
        Review note for {label}
        <input
          value={note}
          maxLength={2_000}
          disabled={disabled}
          onChange={(event) => setNote(event.target.value)}
        />
      </label>
      <div>
        <button
          type="button"
          className="qa-secondary-button"
          disabled={disabled}
          onClick={() => submit("confirmed")}
        >
          Confirm Result
        </button>
        <button
          type="button"
          className="qa-secondary-button"
          disabled={disabled}
          onClick={() => submit("overridden_pass")}
        >
          Override as Pass
        </button>
        <button
          type="button"
          className="qa-secondary-button"
          disabled={disabled}
          onClick={() => submit("overridden_fail")}
        >
          Override as Fail
        </button>
      </div>
    </div>
  );
}
