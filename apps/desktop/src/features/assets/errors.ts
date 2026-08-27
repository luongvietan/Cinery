import type { AppCommandError } from "@cinematic/domain";

function isAppCommandError(value: unknown): value is AppCommandError {
  return (
    typeof value === "object" &&
    value !== null &&
    "message" in value &&
    typeof (value as { message: unknown }).message === "string"
  );
}

/**
 * Formats a caught error (Tauri `AppCommandError`, native `Error`, or
 * anything else) into a user-facing message, mirroring the pattern
 * established by `features/projects/ProjectHome.tsx`.
 */
export function describeCommandError(error: unknown): string {
  if (isAppCommandError(error)) {
    return error.message;
  }
  if (error instanceof Error) {
    return error.message;
  }
  return "Something went wrong. Please try again.";
}
