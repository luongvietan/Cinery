export interface ProjectSummary {
  id: string;
  name: string;
  rootPath: string;
  schemaVersion: number;
  createdAt: string;
  updatedAt: string;
}

export interface CreateProjectInput {
  rootPath: string;
  name: string;
}

export interface OpenProjectInput {
  rootPath: string;
}

export interface RecentProject {
  projectId: string;
  rootPath: string;
  name: string;
  lastOpenedAt: string;
}

export function validateProjectName(value: string): string {
  const trimmed = value.trim();
  if (trimmed.length < 1 || trimmed.length > 120) {
    throw new Error("Project name must contain 1 to 120 characters");
  }
  return trimmed;
}

export function validateProjectRootPath(value: string): string {
  const trimmed = value.trim();
  if (!trimmed) {
    throw new Error("Project path is empty");
  }
  return trimmed;
}
