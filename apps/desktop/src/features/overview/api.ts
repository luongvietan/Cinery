import type { ProjectOverview, RouteProductionIntentResult } from "@cinematic/domain";
import { invokeCommand } from "../../lib/tauri";

export function getProjectOverview(projectRootPath: string): Promise<ProjectOverview> {
  return invokeCommand("get_project_overview", { projectRootPath });
}

export function routeProductionIntent(
  projectRootPath: string,
  text: string,
): Promise<RouteProductionIntentResult> {
  return invokeCommand("route_production_intent", { request: { projectRootPath, text } });
}

/** Count of connected AI services; zero means generation is not possible yet. */
export async function countConnectedAiServices(projectRootPath: string): Promise<number> {
  const providers = await invokeCommand<unknown>("list_custom_providers", { projectRootPath });
  return Array.isArray(providers) ? providers.length : 0;
}
