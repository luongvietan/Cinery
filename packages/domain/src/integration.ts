export const READINESS_STATUSES = ["pending", "complete", "blocked"] as const;

export type ReadinessStatus = (typeof READINESS_STATUSES)[number];

export interface OverviewAction {
  id: string;
  title: string;
  destination: "canon" | "assets" | "production";
}

export interface ReadinessStep {
  id: string;
  title: string;
  status: ReadinessStatus;
  detail: string;
  action: OverviewAction | null;
}

export interface ProjectReadiness {
  status: ReadinessStatus;
  nextAction: OverviewAction | null;
  steps: ReadinessStep[];
}

export interface ProjectHealthSummary {
  openProtectedTbdCount: number;
  openTbdCount: number;
  activeJobCount: number;
}

export interface ActivityItem {
  id: string;
  kind: string;
  label: string;
  occurredAt: string;
}

export interface BackgroundJobSummary {
  id: string;
  operationId: string;
  status: string;
  updatedAt: string;
}

export interface ProjectOverview {
  readiness: ProjectReadiness;
  healthSummary: ProjectHealthSummary;
  recentActivity: ActivityItem[];
  activeJobs: BackgroundJobSummary[];
}
