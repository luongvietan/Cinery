import type {
  CanonEntity,
  CanonEntityDetail,
  CanonEntityType,
  CanonSection,
  CanonSectionRevision,
  CanonTbd,
  CreateCanonEntityInput,
  CreateCanonTbdInput,
  EnsureCanonSingletonsInput,
  ResolveCanonTbdInput,
  ReopenCanonTbdInput,
  SetCanonSectionLockInput,
  UpsertCanonSectionInput,
} from "@cinematic/domain";
import { invokeCommand } from "../../lib/tauri";

export interface CanonSingletonResult {
  story: CanonEntity;
  productionRules: CanonEntity;
}

export interface StoryBibleExportResult {
  relativePath: string;
  byteSize: number;
}

export function ensureCanonSingletons(
  projectRootPath: string,
): Promise<CanonSingletonResult> {
  return invokeCommand("ensure_canon_singletons", { projectRootPath });
}

export function createCanonEntity(input: CreateCanonEntityInput): Promise<CanonEntity> {
  return invokeCommand("create_canon_entity", {
    projectRootPath: input.projectRootPath,
    entityType: input.type,
    name: input.name,
  });
}

export function listCanonEntities(
  projectRootPath: string,
  type?: CanonEntityType,
): Promise<CanonEntity[]> {
  return invokeCommand("list_canon_entities", { projectRootPath, entityType: type });
}

export function getCanonEntity(projectRootPath: string, entityId: string): Promise<CanonEntityDetail> {
  return invokeCommand("get_canon_entity", { projectRootPath, entityId });
}

export function upsertCanonSection(input: UpsertCanonSectionInput): Promise<CanonSection> {
  return invokeCommand("upsert_canon_section", {
    projectRootPath: input.projectRootPath,
    entityId: input.entityId,
    sectionKey: input.sectionKey,
    value: input.value,
    reason: input.reason ?? null,
  });
}

export function lockCanonSection(input: SetCanonSectionLockInput): Promise<CanonSection> {
  return invokeCommand("lock_canon_section", { ...input, reason: input.reason ?? null });
}

export function unlockCanonSection(input: SetCanonSectionLockInput): Promise<CanonSection> {
  return invokeCommand("unlock_canon_section", { ...input, reason: input.reason ?? null });
}

export function listCanonSectionRevisions(
  projectRootPath: string,
  sectionId: string,
): Promise<CanonSectionRevision[]> {
  return invokeCommand("list_canon_section_revisions", { projectRootPath, sectionId });
}

export function createCanonTbd(input: CreateCanonTbdInput): Promise<CanonTbd> {
  return invokeCommand("create_canon_tbd", { ...input });
}

export function listCanonTbds(projectRootPath: string): Promise<CanonTbd[]> {
  return invokeCommand("list_canon_tbds", { projectRootPath });
}

export function resolveCanonTbd(input: ResolveCanonTbdInput): Promise<CanonTbd> {
  return invokeCommand("resolve_canon_tbd", { ...input });
}

export function reopenCanonTbd(input: ReopenCanonTbdInput): Promise<CanonTbd> {
  return invokeCommand("reopen_canon_tbd", { ...input });
}

export function exportStoryBible(projectRootPath: string): Promise<StoryBibleExportResult> {
  return invokeCommand("export_story_bible", { projectRootPath });
}
