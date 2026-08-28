import type { ReactNode } from "react";

interface ActionButtonProps {
  children: ReactNode;
  onClick?: () => void;
  disabled?: boolean;
  /** Rendered when disabled so users can tell why the action is unavailable. */
  disabledReason?: string | null;
  className?: string;
  type?: "button" | "submit";
}

/**
 * Standard action button. When `disabled` is true the reason is rendered
 * right next to the control so a blocked action is always explained
 * (spec: "Do not merely disable a blocked action without explanation").
 */
export function ActionButton({
  children,
  onClick,
  disabled = false,
  disabledReason,
  className,
  type = "button",
}: ActionButtonProps) {
  if (disabled && disabledReason) {
    return (
      <span className="action-button-blocked">
        <button type={type} className={className} disabled onClick={onClick}>
          {children}
        </button>
        <small className="action-button-reason" role="note">
          {disabledReason}
        </small>
      </span>
    );
  }
  return (
    <button
      type={type}
      className={className}
      disabled={disabled}
      onClick={onClick}
    >
      {children}
    </button>
  );
}
