export type CanonSectionStatus = "draft" | "locked";

export interface CanonSection<T = unknown> {
  id: string;
  entityId: string;
  key: string;
  value: T;
  status: CanonSectionStatus;
  revision: number;
  createdAt: string;
  updatedAt: string;
  lockedAt: string | null;
}

export interface PremiseValue {
  text: string;
}

export interface ThesisValue {
  text: string;
}

export interface TimelineEntry {
  id: string;
  label: string;
  description: string;
}

export interface TimelineValue {
  entries: TimelineEntry[];
}

export interface AestheticValue {
  visualRegister: string;
  palette: string[];
  materials: string[];
  lighting: string;
  atmosphere: string;
  exteriorPresence: string;
  anomalyRule: string;
  notes: string[];
}

export interface RelationshipsValue {
  text: string;
}

export interface StructuralEnginesValue {
  engines: Array<{
    id: string;
    title: string;
    description: string;
  }>;
}

export interface ActiveSkillRulesValue {
  text: string;
}

export interface RoleTagValue {
  text: string;
}

export interface VisualSummaryValue {
  text: string;
}

export interface FunctionValue {
  text: string;
}

export interface BackstoryValue {
  text: string;
}

export interface PsychologyValue {
  text: string;
}

export interface PromptReadyDescriptorValue {
  text: string;
}

export interface VisualLock {
  id: string;
  key: string;
  description: string;
  severity: "required" | "important";
  validatorHint: string | null;
}

export interface VisualLocksValue {
  locks: VisualLock[];
}

export interface CharacterSubBeat {
  id: string;
  title: string;
  text: string;
}

export interface SubBeatsValue {
  beats: CharacterSubBeat[];
}

export interface LocationDescriptionValue {
  text: string;
}

export interface VisualTagsValue {
  tags: string[];
}

export interface GeographyValue {
  text: string;
}

export interface LocationRulesValue {
  rules: string[];
}

export interface FactionTextValue {
  text: string;
}

export interface WorldRuleValue {
  text: string;
}

export interface WorldRuleNotesValue {
  text: string;
}

export interface ProductionRule {
  id: string;
  title: string;
  body: string;
}

export interface ProductionRulesValue {
  rules: ProductionRule[];
}

export const CANON_ENTITY_TYPES = [
  "story",
  "character",
  "location",
  "faction",
  "world_rule",
  "production_rules",
] as const;

export type CanonEntityType = (typeof CANON_ENTITY_TYPES)[number];

export interface CanonEntity {
  id: string;
  projectId: string;
  type: CanonEntityType;
  name: string;
  slug: string;
  createdAt: string;
  updatedAt: string;
}

export interface CanonEntityDetail {
  entity: CanonEntity;
  sections: CanonSection[];
}

export interface CreateCanonEntityInput {
  projectRootPath: string;
  type: Exclude<CanonEntityType, "story" | "production_rules">;
  name: string;
}

export interface UpsertCanonSectionInput<T = unknown> {
  projectRootPath: string;
  entityId: string;
  sectionKey: string;
  value: T;
  reason?: string | null;
}

export interface SetCanonSectionLockInput {
  projectRootPath: string;
  sectionId: string;
  reason?: string | null;
}

export interface EnsureCanonSingletonsInput {
  projectRootPath: string;
}

export type CanonChangeKind = "create" | "edit" | "lock" | "unlock";

export interface CanonSectionRevision {
  id: string;
  sectionId: string;
  revision: number;
  value: unknown;
  status: CanonSectionStatus;
  changeKind: CanonChangeKind;
  reason: string | null;
  createdAt: string;
}
