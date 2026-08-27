import type {
  CreateProjectInput,
  OpenProjectInput,
  ProjectSummary,
  RecentProject,
} from "@cinematic/domain";
import { invokeCommand } from "../../lib/tauri";

export function createProject(
  input: CreateProjectInput,
): Promise<ProjectSummary> {
  return invokeCommand<ProjectSummary>("create_project", { ...input });
}

export function openProject(
  input: OpenProjectInput,
): Promise<ProjectSummary> {
  return invokeCommand<ProjectSummary>("open_project", { ...input });
}

export function listRecentProjects(): Promise<RecentProject[]> {
  return invokeCommand<RecentProject[]>("list_recent_projects");
}
