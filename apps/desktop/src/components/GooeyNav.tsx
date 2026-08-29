import { Liquid } from "liquid-gooey";
import type { CSSProperties, ReactNode } from "react";

interface GooeyNavProps {
  children: ReactNode;
  ariaLabel?: string;
}

interface GooeyNavItemProps {
  children?: ReactNode;
  className?: string;
  onClick?: () => void;
  pressed?: boolean;
  label: string;
}

/**
 * Workspace nav rendered as one liquid surface: the pill behind the active
 * entry morphs between entries instead of jumping. The blob color and shadow
 * come from CSS vars so the liquid tracks the theme tokens; the blob must be
 * a solid paint color (SVG filter surface), the active entry re-draws the
 * same color as its own background so the liquid reads as its pill.
 */
export function GooeyNav({ children, ariaLabel }: GooeyNavProps) {
  return (
    <Liquid
      className="gooey-nav"
      blur={7}
      contrast={20}
      fill="var(--goo-nav-blob)"
      shadow="var(--goo-nav-shadow)"
      aria-label={ariaLabel}
    >
      {children}
    </Liquid>
  );
}

export function GooeyNavItem({ children, className, onClick, pressed, label }: GooeyNavItemProps) {
  return (
    <Liquid.Item observe radius={999} className="gooey-nav-item">
      <button
        type="button"
        aria-pressed={pressed}
        className={className}
        onClick={onClick}
        style={{ background: pressed ? "var(--goo-nav-blob)" : undefined } as CSSProperties}
      >
        {label}
        {children}
      </button>
    </Liquid.Item>
  );
}
