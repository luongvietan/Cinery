import type { ProjectOverview } from "@cinematic/domain";
import { invokeCommand } from "../../lib/tauri";

export function getProjectOverview(projectRootPath: string): Promise<ProjectOverview> {
  return invokeCommand("get_project_overview", { projectRootPath });
}
