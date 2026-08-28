import type { ProjectHealthIssue } from "@cinematic/domain";
import { invokeCommand } from "../../lib/tauri";

export function getProjectHealth(projectRootPath: string): Promise<ProjectHealthIssue[]> {
  return invokeCommand("get_project_health", { projectRootPath });
}
