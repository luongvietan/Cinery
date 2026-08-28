export * from "./project";
export * from "./asset";
export * from "./canon";
export * from "./canon-schema";
export * from "./tbd";
export * from "./skill";
export * from "./workflow";
export * from "./execution";
export * from "./generation";
export * from "./lineage";
export * from "./cinema";
export * from "./integration";
export * from "./jobs/mod";

/**
 * Error contract for all app commands.
 * Includes recoverability information for safe restart handling.
 */
export interface AppCommandError {
  code: string;
  message: string;
  recoverability?: "retry" | "resume" | "manual" | "none";
  actionGuidance?: string; // User-facing next steps
}
