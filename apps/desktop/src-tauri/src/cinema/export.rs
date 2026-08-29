use crate::cinema::model::ProviderNeutralCinemaPrompt;
use crate::error::AppError;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

/// Exports the compiled prompt under
/// `project_root/prompts/cinema/{compilationId}.json` atomically (write to
/// a `.tmp` sibling then rename) and returns the project-relative path plus
/// the hex sha256 of the file contents. A human-readable `.md` twin holding
/// the full prompt text is written next to the JSON.
pub fn export_compilation(
    project_root: &Path,
    compilation: &ProviderNeutralCinemaPrompt,
) -> Result<(String, String), AppError> {
    let dir = project_root.join("prompts").join("cinema");
    fs::create_dir_all(&dir).map_err(|e| AppError::FileSystem(e.to_string()))?;

    let json = serde_json::to_string_pretty(compilation)
        .map_err(|e| AppError::FileSystem(e.to_string()))?;

    let json_path = dir.join(format!("{}.json", compilation.compilation_id));
    let tmp_path = dir.join(format!("{}.json.tmp", compilation.compilation_id));
    fs::write(&tmp_path, &json).map_err(|e| AppError::FileSystem(e.to_string()))?;
    fs::rename(&tmp_path, &json_path).map_err(|e| AppError::FileSystem(e.to_string()))?;

    let md_path = dir.join(format!("{}.md", compilation.compilation_id));
    let md_tmp = dir.join(format!("{}.md.tmp", compilation.compilation_id));
    fs::write(&md_tmp, &compilation.provider_prompt)
        .map_err(|e| AppError::FileSystem(e.to_string()))?;
    fs::rename(&md_tmp, &md_path).map_err(|e| AppError::FileSystem(e.to_string()))?;

    let relative = format!("prompts/cinema/{}.json", compilation.compilation_id);

    let mut hasher = Sha256::new();
    hasher.update(json.as_bytes());
    let sha256 = format!("{:x}", hasher.finalize());

    Ok((relative, sha256))
}
