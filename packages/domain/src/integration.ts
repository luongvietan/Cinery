export const READINESS_STATUSES = ["pending", "complete", "blocked"] as const;

export type ReadinessStatus = (typeof READINESS_STATUSES)[number];

export interface OverviewAction {
  id: string;
  title: string;
  destination: "canon" | "assets" | "production" | "cinema";
  characterEntityId: string | null;
  sceneId: string | null;
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

export type HealthSeverity = "info" | "warning" | "error" | "fatal";
export interface ProjectHealthIssue {
  code: string;
  severity: HealthSeverity;
  entityType: string;
  entityId: string | null;
  message: string;
  remediation: string | null;
}

export type ProvenanceKind = "asset_version" | "workflow_run" | "generation" | "qa_run" | "repair_version" | "scene" | "shot" | "cinema_compile";
export interface ProvenanceNode { id: string; kind: ProvenanceKind; label: string; timestamp: string | null; }
export interface ProvenanceEdge { from: string; to: string; relation: string; }
export interface ProvenanceGraph { targetId: string; nodes: ProvenanceNode[]; edges: ProvenanceEdge[]; }

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
  sceneReadiness: SceneReadiness[];
}

export interface SceneReadiness {
  sceneId: string;
  title: string;
  status: ReadinessStatus;
  detail: string;
  action: OverviewAction | null;
}
