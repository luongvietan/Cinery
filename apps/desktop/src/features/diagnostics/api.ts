import type { DiagnosticsBundle } from "@cinematic/domain";
import { invokeCommand } from "../../lib/tauri";

export function exportDiagnostics(projectRootPath: string): Promise<DiagnosticsBundle> {
  return invokeCommand<DiagnosticsBundle>("export_diagnostics", {
    projectRootPath,
  });
}

export function getDiagnosticsFolder(projectRootPath: string): Promise<string> {
  return invokeCommand<string>("get_diagnostics_folder", {
    projectRootPath,
  });
}
