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
 * anything else) into a user-facing message. Shared by every feature that
 * calls a Tauri command via `invokeCommand`, so there is exactly one place
 * that knows how to unwrap an `AppCommandError`.
 */
export function describeError(error: unknown): string {
  if (isAppCommandError(error)) {
    return error.message;
  }
  if (error instanceof Error) {
    return error.message;
  }
  return "Something went wrong. Please try again.";
}
