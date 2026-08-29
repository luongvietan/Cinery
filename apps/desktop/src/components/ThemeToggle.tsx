import { useCallback, useEffect, useState } from "react";

type Theme = "light" | "dark";

function getSystemTheme(): Theme {
  if (typeof window === "undefined" || !window.matchMedia) return "dark";
  return window.matchMedia("(prefers-color-scheme: light)").matches
    ? "light"
    : "dark";
}

function getStoredTheme(): Theme | null {
  try {
    const value = window.localStorage.getItem("cinery-theme");
    return value === "light" || value === "dark" ? value : null;
  } catch {
    return null;
  }
}

function applyTheme(theme: Theme) {
  document.documentElement.dataset.theme = theme;
}

export function ThemeToggle() {
  const [theme, setTheme] = useState<Theme>(
    () => getStoredTheme() ?? getSystemTheme(),
  );

  useEffect(() => {
    applyTheme(theme);
  }, [theme]);

  const toggle = useCallback(() => {
    setTheme((current) => {
      const next = current === "dark" ? "light" : "dark";
      try {
        window.localStorage.setItem("cinery-theme", next);
      } catch {
        // Storage unavailable; theme still applies for this session.
      }
      return next;
    });
  }, []);

  return (
    <button
      type="button"
      className="theme-toggle"
      onClick={toggle}
      aria-label={theme === "dark" ? "Switch to light theme" : "Switch to dark theme"}
      title={theme === "dark" ? "Switch to light theme" : "Switch to dark theme"}
    >
      {theme === "dark" ? (
        <svg viewBox="0 0 16 16" aria-hidden="true">
          <path d="M6.2 1.6a6.4 6.4 0 1 0 8.2 8.2A6.9 6.9 0 0 1 6.2 1.6Z" />
        </svg>
      ) : (
        <svg viewBox="0 0 16 16" aria-hidden="true">
          <path d="M8 4.75a3.25 3.25 0 1 0 0 6.5 3.25 3.25 0 0 0 0-6.5ZM8 1.5a.6.6 0 0 1 .6.6v1.05a.6.6 0 1 1-1.2 0V2.1a.6.6 0 0 1 .6-.6Zm0 10.75a.6.6 0 0 1 .6.6v1.05a.6.6 0 1 1-1.2 0v-1.05a.6.6 0 0 1 .6-.6ZM1.5 8a.6.6 0 0 1 .6-.6h1.05a.6.6 0 1 1 0 1.2H2.1A.6.6 0 0 1 1.5 8Zm10.75 0a.6.6 0 0 1 .6-.6h1.05a.6.6 0 1 1 0 1.2h-1.05a.6.6 0 0 1-.6-.6ZM3.35 3.35a.6.6 0 0 1 .85 0l.74.74a.6.6 0 1 1-.85.85l-.74-.74a.6.6 0 0 1 0-.85Zm6.71 6.71a.6.6 0 0 1 .85 0l.74.74a.6.6 0 1 1-.85.85l-.74-.74a.6.6 0 0 1 0-.85Zm2.59-6.71a.6.6 0 0 1 0 .85l-.74.74a.6.6 0 1 1-.85-.85l.74-.74a.6.6 0 0 1 .85 0ZM3.35 12.65a.6.6 0 0 1 0-.85l.74-.74a.6.6 0 1 1 .85.85l-.74.74a.6.6 0 0 1-.85 0Z" />
        </svg>
      )}
    </button>
  );
}
