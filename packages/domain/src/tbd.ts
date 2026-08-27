export type CanonTbdStatus = "open" | "resolved";

export interface CanonTbd {
  id: string;
  projectId: string;
  canonEntityId: string | null;
  sectionKey: string | null;
  topic: string;
  note: string | null;
  protected: boolean;
  status: CanonTbdStatus;
  resolutionText: string | null;
  createdAt: string;
  updatedAt: string;
  resolvedAt: string | null;
}

export interface CreateCanonTbdInput {
  projectRootPath: string;
  canonEntityId?: string | null;
  sectionKey?: string | null;
  topic: string;
  note?: string | null;
  protected: boolean;
}

export interface ResolveCanonTbdInput {
  projectRootPath: string;
  tbdId: string;
  resolutionText: string;
}

export interface ReopenCanonTbdInput {
  projectRootPath: string;
  tbdId: string;
}
