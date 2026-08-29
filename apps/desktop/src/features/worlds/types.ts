import type { Asset, CanonEntity } from "@cinematic/domain";

export interface World {
  id: string;
  projectId: string;
  canonLocationEntityId: string;
  worldPlateAssetId: string;
  createdAt: string;
  updatedAt: string;
}

export interface WorldDetail {
  world: World;
  location: CanonEntity;
  worldPlateAsset: Asset;
}

export type TbdDecisionKind = "preserve_unknown" | "not_applicable";

export interface TbdDecision {
  tbdId: string;
  topicSnapshot: string;
  noteSnapshot?: string | null;
  decision: TbdDecisionKind;
  justification?: string | null;
}
