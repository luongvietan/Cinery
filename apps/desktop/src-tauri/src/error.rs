use serde::Serialize;

/// Application-level error type shared across all backend modules.
///
/// Add new variants here as later tasks need them; keep each variant's
/// `#[error(...)]` message user-presentable, since it becomes the
/// `AppCommandError.message` seen by the frontend.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Project name must contain 1 to 120 characters")]
    InvalidProjectName,

    #[error("Project path is empty")]
    InvalidProjectPath,

    #[error("Project directory is not empty")]
    ProjectDirectoryNotEmpty,

    #[error("Directory is not an AI Cinematic Production OS project")]
    InvalidProjectDirectory,

    #[error("Project manifest does not match project database")]
    ProjectIdentityMismatch,

    #[error("Filesystem operation failed: {0}")]
    FileSystem(String),

    #[error("Database operation failed: {0}")]
    Database(String),

    #[error("Asset label must contain 1 to 160 characters")]
    InvalidAssetLabel,

    #[error("This asset type is not supported yet")]
    UnsupportedAssetTypeForSprint,

    #[error("Unknown asset type")]
    InvalidAssetType,

    #[error("Asset was not found")]
    AssetNotFound,

    #[error("Asset version was not found")]
    AssetVersionNotFound,

    #[error("Parent version does not belong to the target asset")]
    ParentVersionMismatch,

    #[error("Only PNG, JPEG, and WebP images can be imported in Sprint 1")]
    UnsupportedImageFormat,

    #[error("The video file must be a valid MP4")]
    UnsupportedVideoFormat,

    #[error("This exact media file is already a version of the asset")]
    DuplicateAssetVersion,

    #[error("Image processing failed: {0}")]
    ImageProcessing(String),

    #[error("Canon entity was not found")]
    CanonEntityNotFound,

    #[error("Canon section was not found")]
    CanonSectionNotFound,

    #[error("Canon entity name must contain 1 to 160 characters")]
    InvalidCanonEntityName,

    #[error("This canon entity type is reserved as a project singleton")]
    CanonSingletonTypeRequired,

    #[error("Unknown canon section for this entity type")]
    UnknownCanonSection,

    #[error("Canon section value does not match its schema")]
    InvalidCanonSectionValue,

    #[error("Locked canon sections must be unlocked before editing")]
    CanonSectionLocked,

    #[error("Canon section is already locked")]
    CanonSectionAlreadyLocked,

    #[error("Canon section is already unlocked")]
    CanonSectionAlreadyUnlocked,

    #[error("Canon TBD was not found")]
    CanonTbdNotFound,

    #[error("Canon TBD topic must contain 1 to 240 characters")]
    InvalidCanonTbdTopic,

    #[error("Resolution text must not be blank")]
    InvalidCanonTbdResolution,

    #[error("TBD references a canon entity from another project")]
    CanonTbdEntityProjectMismatch,

    #[error("TBD section key does not exist on the referenced canon entity")]
    CanonTbdSectionMismatch,

    #[error("Story Bible export failed: {0}")]
    CanonExport(String),

    #[error("Skill was not found: {0}")]
    SkillNotFound(String),

    #[error("Skill version was not found: {0}")]
    SkillVersionNotFound(String),

    #[error("Skill operation was not found: {0}")]
    SkillOperationNotFound(String),

    #[error("Builtin skill definition is invalid: {0}")]
    InvalidBuiltinSkillDefinition(String),

    #[error("Workflow input is invalid: {0}")]
    WorkflowInputInvalid(String),

    #[error("Workflow prerequisite failed: {0}")]
    WorkflowPrerequisiteFailed(String),

    #[error("Workflow is blocked by protected TBD: {0}")]
    WorkflowBlockedByProtectedTbd(String),

    #[error("Workflow run was not found: {0}")]
    WorkflowRunNotFound(String),

    #[error("Workflow step was not found: {0}")]
    WorkflowStepNotFound(String),

    #[error("Workflow transition is invalid: {0}")]
    WorkflowInvalidTransition(String),

    #[error("Workflow approval is required")]
    WorkflowApprovalRequired,

    #[error("Workflow approval has already been decided: {0}")]
    WorkflowApprovalAlreadyDecided(String),

    #[error("Workflow run is terminal")]
    WorkflowRunTerminal,

    #[error("Workflow run is inconsistent: {0}")]
    WorkflowRunInconsistent(String),

    #[error("Workflow compiler was not found: {0}")]
    WorkflowCompilerNotFound(String),

    #[error("Workflow resolver was not found: {0}")]
    WorkflowResolverNotFound(String),

    #[error("Workflow executor was not found: {0}")]
    WorkflowExecutorNotFound(String),

    #[error("Workflow artifact could not be written: {0}")]
    WorkflowArtifactWriteFailed(String),

    #[error("Workflow artifact could not be read: {0}")]
    WorkflowArtifactReadFailed(String),

    #[error("Workflow step was interrupted: {0}")]
    InterruptedDuringStep(String),

    #[error("Provider configuration is invalid: {0}")]
    ProviderConfiguration(String),

    #[error("Provider execution failed: {0}")]
    ProviderExecution(String),

    #[error("Generated artifact is unavailable: {0}")]
    GenerationArtifactUnavailable(String),

    #[error("Generated artifact integrity check failed: {0}")]
    GenerationArtifactIntegrityMismatch(String),

    #[error("Generated artifact capture failed: {0}")]
    GenerationArtifactCaptureFailed(String),

    #[error("Generated artifact lineage is incomplete")]
    GenerationLineageIncomplete,

    #[error("Generated artifact belongs to a different project")]
    GenerationProjectMismatch,

    #[error("Generated artifact is not promotable")]
    GenerationArtifactNotPromotable,

    #[error("QA run was not found")]
    QaRunNotFound,

    #[error("QA check was not found")]
    QaCheckNotFound,

    #[error("Visual QA data is invalid: {0}")]
    InvalidQaData(String),

    #[error("Scene was not found")]
    SceneNotFound,

    #[error("Shot was not found")]
    ShotNotFound,

    #[error("Cinema compilation was not found")]
    CinemaCompilationNotFound,

    #[error("Scene title must contain 1 to 160 characters")]
    InvalidSceneTitle,

    #[error("Shot intent must contain 1 to 240 characters")]
    InvalidShotIntent,

    #[error("Cinema duration is invalid: {0}")]
    InvalidCinemaDuration(String),

    #[error("Canon location was not found")]
    WorldLocationNotFound,

    #[error("Canon entity is not a location")]
    WorldLocationInvalidType,

    #[error("A world already exists for this location")]
    WorldAlreadyExists,

    #[error("World was not found")]
    WorldNotFound,

    #[error("World plate asset is invalid: {0}")]
    WorldPlateAssetInvalid(String),

    #[error("TBD decision is required: {0}")]
    TbdDecisionRequired(String),

    #[error("Protected TBD must be preserved: {0}")]
    ProtectedTbdMustBePreserved(String),

    #[error("TBD not applicable reason is required: {0}")]
    TbdNotApplicableReasonRequired(String),

    #[error("Scene summary is invalid: {0}")]
    InvalidSceneSummary(String),

    #[error("World plate has no canonical version: {0}")]
    SceneWorldPlateNotCanonical(String),

    #[error("Character look asset version is not canonical")]
    SceneCharacterLookNotCanonical,

    #[error("Character look is not owned by the given character")]
    SceneCharacterLookNotOwned,

    #[error("Character already assigned to this scene")]
    SceneCharacterAlreadyExists,

    #[error("Character assignment was not found")]
    SceneCharacterNotFound,

    #[error("Character sheet asset version is not canonical")]
    SceneCharacterSheetNotCanonical,

    #[error("Character sheet is not owned by the given character")]
    SceneCharacterSheetNotOwned,

    #[error("Prop plate asset version is not canonical")]
    ScenePropNotCanonical,

    #[error("Prop plate asset type must be prop_plate")]
    ScenePropInvalidType,

    #[error("Prop already assigned to this scene")]
    ScenePropAlreadyExists,

    #[error("Prop assignment was not found")]
    ScenePropNotFound,

    #[error("Scene reference is already current")]
    SceneReferenceAlreadyCurrent,

    #[error("Scene reference has no canonical version: {0}")]
    SceneReferenceCanonicalMissing(String),

    #[error("Scene reference is broken: {0}")]
    SceneReferenceBroken(String),

    #[error("Scene is not ready for keyframe generation: {0}")]
    SceneNotReady(String),

    #[error("Provider capability is not satisfied: {0}")]
    ProviderCapabilityUnsatisfied(String),
}

/// Serializable error shape sent across the Tauri IPC boundary.
///
/// `code` is a stable SCREAMING_SNAKE_CASE identifier derived from the
/// `AppError` variant name, e.g. `PROJECT_DIRECTORY_NOT_EMPTY`.
#[derive(Debug, Serialize)]
pub struct AppCommandError {
    pub code: String,
    pub message: String,
}

impl AppError {
    /// Stable SCREAMING_SNAKE_CASE identifier for this error variant,
    /// mechanically derived from the variant's PascalCase name (e.g.
    /// `ProjectDirectoryNotEmpty` -> `PROJECT_DIRECTORY_NOT_EMPTY`). No
    /// variant is special-cased: a tuple variant's payload is discarded
    /// before the name is converted.
    pub fn code(&self) -> String {
        let debug_repr = format!("{self:?}");
        let variant_name = debug_repr.split('(').next().unwrap_or(&debug_repr);
        to_screaming_snake_case(variant_name)
    }
}

/// Converts a PascalCase identifier (e.g. `ProjectIdentityMismatch`) into
/// SCREAMING_SNAKE_CASE (e.g. `PROJECT_IDENTITY_MISMATCH`) by inserting an
/// underscore before every uppercase letter that isn't the first character.
fn to_screaming_snake_case(variant_name: &str) -> String {
    let mut result = String::with_capacity(variant_name.len() + 4);
    for (index, ch) in variant_name.chars().enumerate() {
        if ch.is_uppercase() && index != 0 {
            result.push('_');
        }
        result.extend(ch.to_uppercase());
    }
    result
}

impl From<AppError> for AppCommandError {
    fn from(error: AppError) -> Self {
        AppCommandError {
            code: error.code(),
            message: error.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_screaming_snake_case_codes_mechanically() {
        assert_eq!(AppError::InvalidProjectName.code(), "INVALID_PROJECT_NAME");
        assert_eq!(AppError::InvalidProjectPath.code(), "INVALID_PROJECT_PATH");
        assert_eq!(
            AppError::ProjectDirectoryNotEmpty.code(),
            "PROJECT_DIRECTORY_NOT_EMPTY"
        );
        assert_eq!(
            AppError::InvalidProjectDirectory.code(),
            "INVALID_PROJECT_DIRECTORY"
        );
        assert_eq!(
            AppError::ProjectIdentityMismatch.code(),
            "PROJECT_IDENTITY_MISMATCH"
        );
        assert_eq!(
            AppError::FileSystem("boom".to_string()).code(),
            "FILE_SYSTEM"
        );
        assert_eq!(AppError::Database("boom".to_string()).code(), "DATABASE");
    }
}
