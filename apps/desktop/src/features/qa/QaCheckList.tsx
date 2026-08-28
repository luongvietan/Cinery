import type { QaCheckRecord, QaCheckStatus, QaReviewStatus } from "./types";
import { QaReviewControls } from "./QaReviewControls";

interface QaCheckListProps {
  title: string;
  ariaLabel: string;
  checks: QaCheckRecord[];
  busyCheckId: string | null;
  onReview: (
    checkId: string,
    status: QaReviewStatus,
    note: string | null,
  ) => Promise<void>;
}

export function effectiveCheckStatus(check: QaCheckRecord): QaCheckStatus {
  if (check.reviewStatus === "overridden_pass") return "pass";
  if (check.reviewStatus === "overridden_fail") return "fail";
  return check.status;
}

function labelFor(check: QaCheckRecord): string {
  if (
    check.requirement &&
    typeof check.requirement === "object" &&
    "label" in check.requirement &&
    typeof check.requirement.label === "string"
  ) {
    return check.requirement.label;
  }
  return check.checkId
    .replace(/^[^:]+:/, "")
    .replace(/_/g, " ")
    .replace(/^./, (letter: string) => letter.toUpperCase());
}

function marker(status: QaCheckStatus): string {
  if (status === "pass") return "✓";
  if (status === "fail") return "×";
  if (status === "uncertain") return "?";
  return "–";
}

export function QaCheckList({
  title,
  ariaLabel,
  checks,
  busyCheckId,
  onReview,
}: QaCheckListProps) {
  if (checks.length === 0) return null;
  return (
    <section className="qa-check-group" aria-label={ariaLabel}>
      <h4>{title}</h4>
      <ul>
        {checks.map((check) => {
          const status = effectiveCheckStatus(check);
          const label = labelFor(check);
          return (
            <li key={check.checkId} className={`qa-check qa-check--${status}`}>
              <span className="qa-check-marker" aria-hidden="true">
                {marker(status)}
              </span>
              <div className="qa-check-copy">
                <div className="qa-check-heading">
                  <strong>{label}</strong>
                  <span>{status.replace("_", " ")}</span>
                  {check.confidence === null ? null : (
                    <span>{Math.round(check.confidence * 100)}% confidence</span>
                  )}
                </div>
                <p>{check.observed}</p>
                <p className="qa-check-reason">{check.reason}</p>
                {check.reviewStatus === "unreviewed" ? null : (
                  <p className="qa-review-state">
                    Human review: {check.reviewStatus.replace(/_/g, " ")}
                    {check.reviewNote ? `, ${check.reviewNote}` : ""}
                  </p>
                )}
                <QaReviewControls
                  label={label}
                  disabled={busyCheckId !== null}
                  initialNote={check.reviewNote}
                  onReview={(reviewStatus, note) =>
                    onReview(check.checkId, reviewStatus, note)
                  }
                />
              </div>
            </li>
          );
        })}
      </ul>
    </section>
  );
}
