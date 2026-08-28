import type { AssetVersion, GenerationResultSetDetail } from "@cinematic/domain";
import { invokeCommand } from "../../lib/tauri";

export function listGenerationResults(
  projectRootPath: string,
  workflowRunId?: string,
): Promise<GenerationResultSetDetail[]> {
  return invokeCommand("list_generation_results", {
    projectRootPath,
    workflowRunId,
  });
}

export function getGeneratedArtifact(
  projectRootPath: string,
  artifactId: string,
): Promise<GenerationResultSetDetail["artifacts"][number]> {
  return invokeCommand("get_generated_artifact", { projectRootPath, artifactId });
}

export function promoteGeneratedArtifact(
  projectRootPath: string,
  artifactId: string,
  targetAssetId: string,
  setCanonical: boolean,
): Promise<AssetVersion> {
  return invokeCommand("promote_generated_artifact", {
    projectRootPath,
    artifactId,
    targetAssetId,
    setCanonical,
  });
}
