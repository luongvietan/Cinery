import type {
  ProviderCapabilities,
  ProviderConfigurationStatus,
  CustomProviderDefinition,
  ProviderConnectionTestResult,
  ProviderPreset,
  SkillOperation,
  WorkflowCharacterOption,
  WorkflowRunDetail,
  WorkflowRunRecord,
} from "@cinematic/domain";
import { invokeCommand } from "../../lib/tauri";

export function listSkillOperations(): Promise<SkillOperation[]> {
  return invokeCommand("list_skill_operations");
}

export function listWorkflowCharacters(projectRootPath: string): Promise<WorkflowCharacterOption[]> {
  return invokeCommand("list_workflow_characters", { projectRootPath });
}

export function createWorkflowRun(
  projectRootPath: string,
  skillId: string,
  skillVersion: string,
  operationId: string,
  input: Record<string, unknown>,
): Promise<WorkflowRunDetail> {
  return invokeCommand("create_workflow_run", {
    projectRootPath,
    skillId,
    skillVersion,
    operationId,
    input,
  });
}

export function advanceWorkflowRun(
  projectRootPath: string,
  workflowRunId: string,
): Promise<WorkflowRunDetail> {
  return invokeCommand("advance_workflow_run", { projectRootPath, workflowRunId });
}

export function approveWorkflowStep(
  projectRootPath: string,
  workflowRunId: string,
  stepDefinitionId: string,
  note: string | null,
): Promise<WorkflowRunDetail> {
  return invokeCommand("approve_workflow_step", { projectRootPath, workflowRunId, stepDefinitionId, note });
}

export function rejectWorkflowStep(
  projectRootPath: string,
  workflowRunId: string,
  stepDefinitionId: string,
  note: string | null,
): Promise<WorkflowRunDetail> {
  return invokeCommand("reject_workflow_step", { projectRootPath, workflowRunId, stepDefinitionId, note });
}

export function cancelWorkflowRun(projectRootPath: string, workflowRunId: string): Promise<WorkflowRunDetail> {
  return invokeCommand("cancel_workflow_run", { projectRootPath, workflowRunId });
}

export function getWorkflowRun(projectRootPath: string, workflowRunId: string): Promise<WorkflowRunDetail> {
  return invokeCommand("get_workflow_run", { projectRootPath, workflowRunId });
}

export function listWorkflowRuns(projectRootPath: string): Promise<WorkflowRunRecord[]> {
  return invokeCommand("list_workflow_runs", { projectRootPath });
}

export function listProviders(projectRootPath?: string): Promise<string[]> {
  return invokeCommand("list_providers", projectRootPath ? { projectRootPath } : undefined);
}

export function listProviderPresets(): Promise<ProviderPreset[]> {
  return invokeCommand("list_provider_presets", undefined);
}

export function listCustomProviders(projectRootPath: string): Promise<CustomProviderDefinition[]> {
  return invokeCommand("list_custom_providers", { projectRootPath });
}

export function upsertCustomProvider(projectRootPath: string, definition: CustomProviderDefinition): Promise<CustomProviderDefinition> {
  return invokeCommand("upsert_custom_provider", { projectRootPath, definition });
}

export function deleteCustomProvider(projectRootPath: string, providerId: string): Promise<void> {
  return invokeCommand("delete_custom_provider", { projectRootPath, providerId });
}

export function testCustomProviderConnection(projectRootPath: string, providerId: string): Promise<ProviderConnectionTestResult> {
  return invokeCommand("test_custom_provider_connection", { projectRootPath, providerId });
}

export function getProviderCapabilities(projectRootPath: string, providerId: string): Promise<ProviderCapabilities> {
  return invokeCommand("get_provider_capabilities", { projectRootPath, providerId });
}

export function suggestVisualSpec(
  projectRootPath: string,
  characterName: string,
  notes: string,
): Promise<Record<string, string>> {
  return invokeCommand("suggest_visual_spec", { projectRootPath, characterName, notes });
}

export function getProviderConfigurationStatus(projectRootPath: string, providerId: string): Promise<ProviderConfigurationStatus> {
  return invokeCommand("get_provider_configuration_status", { projectRootPath, providerId });
}

export function configureProvider(projectRootPath: string, config: Record<string, unknown>): Promise<ProviderConfigurationStatus> {
  return invokeCommand("configure_provider", { projectRootPath, config });
}

export function saveProviderCredential(projectRootPath: string, providerId: string, secret: string, defaultModel: string | null): Promise<ProviderConfigurationStatus> {
  return invokeCommand("save_provider_credential", { projectRootPath, providerId, secret, defaultModel });
}

export function removeProviderCredentials(projectRootPath: string, providerId: string): Promise<void> {
  return invokeCommand("remove_provider_credentials", { projectRootPath, providerId });
}

export function validateProviderConfiguration(projectRootPath: string, providerId: string): Promise<void> {
  return invokeCommand("validate_provider_configuration", { projectRootPath, providerId });
}

export function listProviderModels(providerId: string, projectRootPath?: string): Promise<string[]> {
  return invokeCommand("list_provider_models", projectRootPath ? { providerId, projectRootPath } : { providerId });
}

export function cancelWorkflowExecution(projectRootPath: string, workflowRunId: string, stepId: string): Promise<WorkflowRunDetail> {
  return invokeCommand("cancel_workflow_execution", { projectRootPath, workflowRunId, stepId });
}

export function retryWorkflowExecution(projectRootPath: string, workflowRunId: string, stepId: string): Promise<WorkflowRunDetail> {
  return invokeCommand("retry_workflow_execution", { projectRootPath, workflowRunId, stepId });
}
