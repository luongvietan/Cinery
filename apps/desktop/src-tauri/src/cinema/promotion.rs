//! Idempotent, conflict-safe Shot video promotion (P10.2).
//!
//! One exact captured `shot.image_to_video` candidate — already imported as
//! a candidate AssetVersion of the scene-owned video asset — is promoted
//! under explicit human review. The Shot pin update is a nullable
//! compare-and-set against the caller's expected current pin so two
//! conflicting candidates can never both silently win, and replaying the
//! winner (crash reconciliation) returns the same exact version.
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::db;
use crate::error::AppError;
use crate::generation::service::GenerationService;
use crate::project::service::ProjectService;

/// The result of pinning one exact promoted candidate version onto a Shot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShotVideoPromotionResult {
    pub shot_id: String,
    pub artifact_id: String,
    pub asset_version_id: String,
    pub previous_asset_version_id: Option<String>,
}

/// Nullable compare-and-set for the Shot's exact generated-video pin,
/// scoped through the Shot's Scene/project. Returns `true` when the row was
/// updated (the caller's expected pin matched) and `false` on zero rows.
pub fn set_shot_video_if_current(
    tx: &rusqlite::Transaction<'_>,
    project_id: &str,
    shot_id: &str,
    expected_current: Option<&str>,
    next_version: &str,
) -> Result<bool, AppError> {
    let updated = tx
        .execute(
            "UPDATE scene_shots
             SET generated_video_asset_version_id = ?1, updated_at = ?2
             WHERE id = ?3
               AND scene_id IN (SELECT id FROM world_scenes WHERE project_id = ?4)
               AND ((generated_video_asset_version_id IS NULL AND ?5 IS NULL)
                    OR generated_video_asset_version_id = ?5)",
            params![
                next_version,
                chrono::Utc::now().to_rfc3339(),
                shot_id,
                project_id,
                expected_current
            ],
        )
        .map_err(|error| AppError::Database(error.to_string()))?;
    Ok(updated > 0)
}

/// Promotes one exact captured I2V candidate onto the Shot's video pin.
///
/// Validation order per design: artifact availability/kind, lineage, the
/// producing run's operation + input Shot/Scene, the exact frozen source
/// keyframe, the scene-owned video asset, then a preflight of the caller's
/// expected pin. Artifact/version promotion itself is idempotent
/// (content-deduped import), so a crash between phases replays safely; the
/// pin update is a nullable compare-and-set inside one immediate
/// transaction, and the `shot.video.promoted` audit event is appended only
/// when the pin actually changed.
pub fn promote_shot_video_candidate(
    project_root: &Path,
    shot_id: &str,
    artifact_id: &str,
    expected_current_video_asset_version_id: Option<&str>,
) -> Result<ShotVideoPromotionResult, AppError> {
    let project = ProjectService::open(project_root)?;
    let project_id = project.id.clone();

    // The initiating Shot must exist in this project.
    let conn = db::open_existing_connection(&project_root.join("project.db"))?;
    let shot_scene_id: String = conn
        .query_row(
            "SELECT ss.scene_id FROM scene_shots ss \
             JOIN world_scenes ws ON ws.id = ss.scene_id \
             WHERE ss.id = ?1 AND ws.project_id = ?2",
            params![shot_id, project_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| AppError::Database(error.to_string()))?
        .ok_or(AppError::ShotNotFound)?;

    // Available video output only.
    let detail = GenerationService::get_artifact_detail(project_root, artifact_id)?;
    if detail.artifact.capture_status != "available"
        || detail.artifact.media_kind != "video"
        || detail.artifact.mime_type != "video/mp4"
    {
        return Err(AppError::GenerationArtifactNotPromotable);
    }
    let lineage = detail
        .lineage
        .ok_or(AppError::GenerationLineageIncomplete)?;

    // The producing run must be the I2V run naming this exact Shot/Scene.
    let (operation_id, input_json): (String, String) = conn
        .query_row(
            "SELECT operation_id, input_json FROM workflow_runs WHERE id = ?1",
            params![lineage.workflow_run_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| AppError::Database(error.to_string()))?
        .ok_or(AppError::GenerationLineageIncomplete)?;
    if operation_id != "shot.image_to_video" {
        return Err(AppError::GenerationArtifactNotPromotable);
    }
    let input: serde_json::Value = serde_json::from_str(&input_json)
        .map_err(|error| AppError::WorkflowRunInconsistent(error.to_string()))?;
    let run_scene_id = input.get("sceneId").and_then(serde_json::Value::as_str);
    let run_shot_id = input.get("shotId").and_then(serde_json::Value::as_str);
    if run_scene_id != Some(shot_scene_id.as_str()) || run_shot_id != Some(shot_id) {
        return Err(AppError::GenerationArtifactNotPromotable);
    }

    // Lineage must contain the exact source keyframe frozen into the
    // producing run -- not the Shot's current pin, which may have drifted
    // after submission under explicit human review.
    let Some(source_version_id) = input
        .get("sourceAssetVersionId")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
    else {
        return Err(AppError::SourceKeyframeMissing);
    };
    if !lineage
        .source_asset_version_ids
        .iter()
        .any(|id| id == &source_version_id)
    {
        return Err(AppError::GenerationArtifactNotPromotable);
    }

    // Preflight the caller's expected pin before doing any work: a stale
    // or conflicting expectation loses immediately, without importing or
    // promoting anything.
    let current_pin: Option<String> = conn
        .query_row(
            "SELECT generated_video_asset_version_id FROM scene_shots WHERE id = ?1",
            params![shot_id],
            |row| row.get(0),
        )
        .map_err(|error| AppError::Database(error.to_string()))?;
    if current_pin.as_deref() != expected_current_video_asset_version_id {
        return Err(AppError::PromotionConflict);
    }

    // Resolve the scene-owned video asset (find semantics: one video asset
    // per scene holds every run — matches completion-time import).
    let video_asset_id: String = conn
        .query_row(
            "SELECT id FROM assets \
             WHERE project_id = ?1 AND type = 'video' AND owner_entity_id = ?2 \
             ORDER BY created_at ASC, id ASC LIMIT 1",
            params![project_id, shot_scene_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| AppError::Database(error.to_string()))?
        .ok_or(AppError::AssetNotFound)?;

    // Idempotent artifact -> AssetVersion promotion (content-deduped import
    // returns the exact already-imported candidate version).
    let promoted = GenerationService::promote_generated_artifact(
        project_root,
        artifact_id,
        &video_asset_id,
        true,
    )?;

    let mut conn = db::open_existing_connection(&project_root.join("project.db"))?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| AppError::Database(error.to_string()))?;

    // Crash reconciliation: the pin may already hold the promoted version.
    let pin_in_tx: Option<String> = tx
        .query_row(
            "SELECT generated_video_asset_version_id FROM scene_shots WHERE id = ?1",
            params![shot_id],
            |row| row.get(0),
        )
        .map_err(|error| AppError::Database(error.to_string()))?;
    if pin_in_tx.as_deref() == Some(promoted.id.as_str()) {
        tx.commit()
            .map_err(|error| AppError::Database(error.to_string()))?;
        return Ok(ShotVideoPromotionResult {
            shot_id: shot_id.to_string(),
            artifact_id: artifact_id.to_string(),
            asset_version_id: promoted.id.clone(),
            previous_asset_version_id: expected_current_video_asset_version_id.map(str::to_string),
        });
    }

    let changed = set_shot_video_if_current(
        &tx,
        &project_id,
        shot_id,
        expected_current_video_asset_version_id,
        &promoted.id,
    )?;
    if !changed {
        return Err(AppError::PromotionConflict);
    }
    crate::providers::repository::append_audit_event(
        &tx,
        Some(&lineage.provider_attempt_id),
        &lineage.workflow_run_id,
        "shot.video.promoted",
        Some(&serde_json::json!({
            "shotId": shot_id,
            "sceneId": shot_scene_id,
            "sourceAssetVersionId": source_version_id,
            "workflowRunId": lineage.workflow_run_id,
            "providerAttemptId": lineage.provider_attempt_id,
            "artifactId": artifact_id,
            "assetVersionId": promoted.id,
            "previousAssetVersionId": expected_current_video_asset_version_id,
        })),
    )?;
    tx.commit()
        .map_err(|error| AppError::Database(error.to_string()))?;

    Ok(ShotVideoPromotionResult {
        shot_id: shot_id.to_string(),
        artifact_id: artifact_id.to_string(),
        asset_version_id: promoted.id,
        previous_asset_version_id: expected_current_video_asset_version_id.map(str::to_string),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::service::AssetService;
    use crate::cinema::model::ShotRecord;
    use crate::cinema::service::CinemaService;
    use crate::generation::model::GeneratedArtifact;
    use crate::generation::service::{GenerationCaptureInput, GenerationService};
    use crate::providers::model::{ProviderOutput, ProviderResult};
    use rusqlite::params;
    use serde_json::json;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// Minimal ISO-BMFF (ftyp) payload: capture and import only verify the
    /// MP4 magic, so these deterministic bytes stand in for a real video.
    fn mp4_bytes(seed: u8) -> Vec<u8> {
        let mut bytes = vec![0x00, 0x00, 0x00, 0x18];
        bytes.extend_from_slice(b"ftypmp42");
        bytes.extend_from_slice(&[0x00, 0x00, 0x00, seed]);
        bytes.extend_from_slice(b"mp42isom");
        bytes
    }

    fn png_bytes() -> Vec<u8> {
        let image: image::RgbaImage =
            image::ImageBuffer::from_pixel(8, 8, image::Rgba([10, 20, 30, 255]));
        let mut cursor = std::io::Cursor::new(Vec::new());
        image
            .write_to(&mut cursor, image::ImageFormat::Png)
            .unwrap();
        cursor.into_inner()
    }

    struct CompletedShot {
        _temp: TempDir,
        root: PathBuf,
        scene_id: String,
        shot_id: String,
        source_version_id: String,
        artifact_id: String,
        video_asset_id: String,
    }

    impl CompletedShot {
        fn shot(&self) -> ShotRecord {
            CinemaService::list_shots(&self.root, &self.scene_id)
                .unwrap()
                .into_iter()
                .find(|shot| shot.id == self.shot_id)
                .unwrap()
        }

        fn promote(&self, expected: Option<&str>) -> Result<ShotVideoPromotionResult, AppError> {
            self.promote_with(&self.artifact_id, expected)
        }

        fn promote_with(
            &self,
            artifact_id: &str,
            expected: Option<&str>,
        ) -> Result<ShotVideoPromotionResult, AppError> {
            promote_shot_video_candidate(&self.root, &self.shot_id, artifact_id, expected)
        }

        fn project_id(&self) -> String {
            let conn = db::open_existing_connection(&self.root.join("project.db")).unwrap();
            conn.query_row("SELECT id FROM projects", [], |row| row.get(0))
                .unwrap()
        }

        /// Captures one more video artifact for the same run through a fresh
        /// provider attempt (`generation_result_sets.provider_attempt_id` is
        /// unique), returning its artifact id.
        fn capture_extra_video_artifact(&self, attempt_id: &str, seed: u8) -> String {
            let conn = db::open_existing_connection(&self.root.join("project.db")).unwrap();
            conn.execute(
                "INSERT INTO workflow_step_executions (id, workflow_run_id, step_definition_id, \
                 attempt_number, compiled_request_id, provider_id, model_id, adapter_version, \
                 idempotency_key, status, started_at) \
                 VALUES (?1, 'run-i2v', 'execute', 2, 'compiled-i2v-2', 'fake_async_video', \
                 'fake-video-v1', 1, 'run-i2v:execute:2', 'succeeded', 'now')",
                [attempt_id],
            )
            .unwrap();
            GenerationService::capture_provider_result(
                &self.root,
                &GenerationCaptureInput {
                    project_id: self.project_id(),
                    workflow_run_id: "run-i2v".into(),
                    workflow_step_key: "execute".into(),
                    workflow_definition_id: "scene-builder".into(),
                    workflow_version: "1.0.0".into(),
                    skill_id: "scene-builder".into(),
                    skill_version: "1.0.0".into(),
                    compiled_execution_artifact_id: "compiled-i2v-2".into(),
                    compiled_request_sha256: "d".repeat(64),
                    canon_snapshot_id: None,
                    canon_snapshot_sha256: None,
                    provider_attempt_id: attempt_id.into(),
                    provider_id: "fake_async_video".into(),
                    model_id: "fake-video-v1".into(),
                    source_asset_version_ids: vec![self.source_version_id.clone()],
                    requested_output_count: 1,
                    media_kind: "video".into(),
                },
                &video_result(seed, "shot-video-2.mp4"),
            )
            .unwrap()
            .artifacts[0]
                .id
                .clone()
        }

        /// Captures an image artifact under a fresh attempt + a separate
        /// `scene.generate_video` run, standing in for an artifact that is
        /// not a shot I2V video output.
        fn capture_wrong_operation_image_artifact(&self, attempt_id: &str) -> String {
            self.insert_run(
                "run-other",
                "scene.generate_video",
                &format!("{{\"sceneId\":\"{}\"}}", self.scene_id),
            );
            self.capture(attempt_id, "run-other", "image", "e".repeat(64))
        }

        /// Captures an artifact whose lineage is intact but whose producing
        /// run names a different Shot, standing in for cross-shot misuse.
        fn capture_wrong_shot_artifact(&self, attempt_id: &str, other_shot_id: &str) -> String {
            self.insert_run(
                "run-wrong-shot",
                "shot.image_to_video",
                &format!(
                    "{{\"sceneId\":\"{}\",\"shotId\":\"{}\",\"sourceAssetVersionId\":\"{}\"}}",
                    self.scene_id, other_shot_id, self.source_version_id
                ),
            );
            self.capture(attempt_id, "run-wrong-shot", "video", "f".repeat(64))
        }

        fn insert_run(&self, run_id: &str, operation_id: &str, input_json: &str) {
            let conn = db::open_existing_connection(&self.root.join("project.db")).unwrap();
            conn.execute(
                "INSERT INTO workflow_runs (id, project_id, skill_id, skill_version, operation_id, \
                 status, input_json, created_at, updated_at) \
                 VALUES (?1, ?2, 'scene-builder', '1.0.0', ?3, 'completed', ?4, 'now', 'now')",
                params![run_id, self.project_id(), operation_id, input_json],
            )
            .unwrap();
        }

        fn capture(
            &self,
            attempt_id: &str,
            run_id: &str,
            media_kind: &str,
            compiled_sha: String,
        ) -> String {
            let conn = db::open_existing_connection(&self.root.join("project.db")).unwrap();
            conn.execute(
                "INSERT INTO workflow_step_executions (id, workflow_run_id, step_definition_id, \
                 attempt_number, compiled_request_id, provider_id, model_id, adapter_version, \
                 idempotency_key, status, started_at) \
                 VALUES (?1, ?2, 'execute', 2, 'compiled-extra', 'fake_async_video', \
                 'fake-video-v1', 1, ?2 || ':execute:2', 'succeeded', 'now')",
                [attempt_id, run_id],
            )
            .unwrap();
            let result = if media_kind == "video" {
                video_result(9, "extra.mp4")
            } else {
                let png = png_bytes();
                ProviderResult {
                    outputs: vec![ProviderOutput {
                        uri: format!(
                            "data:image/png;base64,{}",
                            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, png)
                        ),
                        mime_type: "image/png".into(),
                        filename: Some("extra.png".into()),
                    }],
                    provider_reported_model: Some("fake-video-v1".into()),
                    metadata: json!({}),
                }
            };
            GenerationService::capture_provider_result(
                &self.root,
                &GenerationCaptureInput {
                    project_id: self.project_id(),
                    workflow_run_id: run_id.into(),
                    workflow_step_key: "execute".into(),
                    workflow_definition_id: "scene-builder".into(),
                    workflow_version: "1.0.0".into(),
                    skill_id: "scene-builder".into(),
                    skill_version: "1.0.0".into(),
                    compiled_execution_artifact_id: "compiled-extra".into(),
                    compiled_request_sha256: compiled_sha,
                    canon_snapshot_id: None,
                    canon_snapshot_sha256: None,
                    provider_attempt_id: attempt_id.into(),
                    provider_id: "fake_async_video".into(),
                    model_id: "fake-video-v1".into(),
                    source_asset_version_ids: vec![self.source_version_id.clone()],
                    requested_output_count: 1,
                    media_kind: media_kind.into(),
                },
                &result,
            )
            .unwrap()
            .artifacts[0]
                .id
                .clone()
        }
    }

    fn video_result(seed: u8, filename: &str) -> ProviderResult {
        ProviderResult {
            outputs: vec![ProviderOutput {
                uri: format!(
                    "data:video/mp4;base64,{}",
                    base64::Engine::encode(
                        &base64::engine::general_purpose::STANDARD,
                        mp4_bytes(seed)
                    )
                ),
                mime_type: "video/mp4".into(),
                filename: Some(filename.into()),
            }],
            provider_reported_model: Some("fake-video-v1".into()),
            metadata: json!({}),
        }
    }

    /// A project with a scene + shot whose pinned keyframe was "animated" by
    /// a completed `shot.image_to_video` run: durable run/attempt rows, a
    /// captured video artifact with full lineage, and the imported candidate
    /// version in the scene-owned video asset.
    fn completed_shot_i2v_fixture() -> CompletedShot {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("shot-promotion");
        ProjectService::create(&root, "Shot Promotion").unwrap();

        let scene = crate::scenes::commands::create_world_scene(
            root.to_string_lossy().to_string(),
            "Scene 001".into(),
            "Ops room stand-off".into(),
        )
        .unwrap();
        let shot = CinemaService::create_shot(&root, &scene.id, None, 4.0, "Establish", None, None)
            .unwrap();
        let keyframe_asset =
            crate::scenes::service::SceneService::ensure_scene_keyframe_asset(&root, &scene.id)
                .unwrap();
        let source = root.join("keyframe.png");
        let image: image::RgbaImage =
            image::ImageBuffer::from_pixel(8, 8, image::Rgba([29, 47, 83, 255]));
        image.save(&source).unwrap();
        let keyframe_version =
            AssetService::import_asset_version(&root, &keyframe_asset.id, &source, None).unwrap();
        AssetService::promote_asset_version(&root, &keyframe_version.id).unwrap();
        CinemaService::set_shot_keyframe(&root, &shot.id, Some(&keyframe_version.id)).unwrap();

        // Durable rows for the completed shot.image_to_video run.
        let conn = db::open_existing_connection(&root.join("project.db")).unwrap();
        let project_id: String = conn
            .query_row("SELECT id FROM projects", [], |row| row.get(0))
            .unwrap();
        let input = json!({
            "sceneId": scene.id,
            "shotId": shot.id,
            "sourceAssetVersionId": keyframe_version.id,
        });
        conn.execute(
            "INSERT INTO workflow_runs (id, project_id, skill_id, skill_version, operation_id, \
             status, input_json, created_at, updated_at) \
             VALUES ('run-i2v', ?1, 'scene-builder', '1.0.0', 'shot.image_to_video', \
             'completed', ?2, 'now', 'now')",
            params![project_id, input.to_string()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO workflow_step_executions (id, workflow_run_id, step_definition_id, \
             attempt_number, compiled_request_id, provider_id, model_id, adapter_version, \
             idempotency_key, status, started_at) \
             VALUES ('attempt-i2v', 'run-i2v', 'execute', 1, 'compiled-i2v', 'fake_async_video', \
             'fake-video-v1', 1, 'run-i2v:execute:1', 'succeeded', 'now')",
            [],
        )
        .unwrap();

        // Capture the video artifact with full lineage from the frozen source.
        let artifacts = GenerationService::capture_provider_result(
            &root,
            &GenerationCaptureInput {
                project_id,
                workflow_run_id: "run-i2v".into(),
                workflow_step_key: "execute".into(),
                workflow_definition_id: "scene-builder".into(),
                workflow_version: "1.0.0".into(),
                skill_id: "scene-builder".into(),
                skill_version: "1.0.0".into(),
                compiled_execution_artifact_id: "compiled-i2v".into(),
                compiled_request_sha256: "b".repeat(64),
                canon_snapshot_id: None,
                canon_snapshot_sha256: None,
                provider_attempt_id: "attempt-i2v".into(),
                provider_id: "fake_async_video".into(),
                model_id: "fake-video-v1".into(),
                source_asset_version_ids: vec![keyframe_version.id.clone()],
                requested_output_count: 1,
                media_kind: "video".into(),
            },
            &video_result(3, "shot-video.mp4"),
        )
        .unwrap()
        .artifacts;
        assert_eq!(artifacts.len(), 1);
        let artifact_id = artifacts[0].id.clone();

        // The scene-owned video asset holding the imported candidate version
        // (what workflow completion already did before promotion exists).
        let video_asset =
            AssetService::create_asset(&root, "video", "Scene 001 video", Some(scene.id.clone()))
                .unwrap();
        let candidate_source = root.join("candidate.mp4");
        std::fs::write(&candidate_source, mp4_bytes(3)).unwrap();
        let candidate =
            AssetService::import_media_version(&root, &video_asset.id, &candidate_source, None)
                .unwrap();
        assert_eq!(candidate.sha256, artifacts[0].sha256);

        CompletedShot {
            _temp: temp,
            root,
            scene_id: scene.id,
            shot_id: shot.id,
            source_version_id: keyframe_version.id,
            artifact_id,
            video_asset_id: video_asset.id,
        }
    }

    fn audit_payloads(root: &PathBuf, event_type: &str) -> Vec<serde_json::Value> {
        let conn = db::open_existing_connection(&root.join("project.db")).unwrap();
        let mut statement = conn
            .prepare(
                "SELECT payload_json FROM provider_audit_events \
                 WHERE event_type = ?1 ORDER BY created_at ASC",
            )
            .unwrap();
        let rows = statement
            .query_map([event_type], |row| row.get::<_, String>(0))
            .unwrap();
        rows.map(|row| serde_json::from_str(&row.unwrap()).unwrap())
            .collect()
    }

    #[test]
    fn promotes_exact_i2v_candidate_and_keeps_source_keyframe() {
        let fixture = completed_shot_i2v_fixture();
        let before = fixture.shot();
        let promoted = fixture
            .promote(before.generated_video_asset_version_id.as_deref())
            .unwrap();

        assert_eq!(promoted.shot_id, fixture.shot_id);
        assert_eq!(promoted.artifact_id, fixture.artifact_id);
        assert_eq!(
            promoted.previous_asset_version_id.as_deref(),
            before.generated_video_asset_version_id.as_deref()
        );

        let after = fixture.shot();
        // The exact candidate wins the pin; the source keyframe is untouched.
        assert_eq!(
            after.generated_video_asset_version_id,
            Some(promoted.asset_version_id.clone())
        );
        assert_eq!(
            after.keyframe_asset_version_id,
            before.keyframe_asset_version_id
        );
        assert_eq!(
            after.keyframe_asset_version_id.as_deref(),
            Some(fixture.source_version_id.as_str())
        );

        // The promoted version is the exact captured candidate version.
        let asset =
            AssetService::get_asset_with_versions(&fixture.root, &fixture.video_asset_id).unwrap();
        let version = asset
            .versions
            .iter()
            .find(|version| version.id == promoted.asset_version_id)
            .unwrap();
        assert_eq!(version.sha256, fixture_artifact_sha(&fixture));

        // Provenance: one audit event with shot/source/run/attempt/artifact ids.
        let payloads = audit_payloads(&fixture.root, "shot.video.promoted");
        assert_eq!(payloads.len(), 1);
        assert_eq!(payloads[0]["shotId"], fixture.shot_id);
        assert_eq!(payloads[0]["sceneId"], fixture.scene_id);
        assert_eq!(
            payloads[0]["sourceAssetVersionId"],
            fixture.source_version_id
        );
        assert_eq!(payloads[0]["workflowRunId"], "run-i2v");
        assert_eq!(payloads[0]["providerAttemptId"], "attempt-i2v");
        assert_eq!(payloads[0]["artifactId"], fixture.artifact_id);
        assert_eq!(payloads[0]["assetVersionId"], promoted.asset_version_id);
        assert_eq!(
            payloads[0]["previousAssetVersionId"],
            serde_json::to_value(&before.generated_video_asset_version_id).unwrap()
        );
    }

    fn fixture_artifact_sha(fixture: &CompletedShot) -> String {
        let conn = db::open_existing_connection(&fixture.root.join("project.db")).unwrap();
        conn.query_row(
            "SELECT sha256 FROM generated_artifacts WHERE id = ?1",
            [&fixture.artifact_id],
            |row| row.get(0),
        )
        .unwrap()
    }

    #[test]
    fn conflicting_shot_promotion_is_rejected_without_repinning() {
        let fixture = completed_shot_i2v_fixture();
        let second_artifact = fixture.capture_extra_video_artifact("attempt-i2v-2", 11);
        let first = fixture.promote(None).unwrap();

        let error = fixture.promote_with(&second_artifact, None).unwrap_err();
        assert!(matches!(error, AppError::PromotionConflict));

        // The winner's pin is untouched; the loser never repinned anything.
        assert_eq!(
            fixture.shot().generated_video_asset_version_id,
            Some(first.asset_version_id)
        );
        assert_eq!(
            audit_payloads(&fixture.root, "shot.video.promoted").len(),
            1
        );
    }

    #[test]
    fn replaying_the_winning_promotion_is_idempotent() {
        let fixture = completed_shot_i2v_fixture();
        let first = fixture.promote(None).unwrap();

        let replay = fixture
            .promote_with(&fixture.artifact_id, Some(&first.asset_version_id))
            .unwrap();
        assert_eq!(replay.asset_version_id, first.asset_version_id);
        assert_eq!(
            fixture.shot().generated_video_asset_version_id,
            Some(first.asset_version_id)
        );
        // No duplicate provenance for the replayed no-op.
        assert_eq!(
            audit_payloads(&fixture.root, "shot.video.promoted").len(),
            1
        );
    }

    #[test]
    fn stale_expected_pin_is_rejected_before_any_work() {
        let fixture = completed_shot_i2v_fixture();
        let error = fixture
            .promote_with(&fixture.artifact_id, Some("stale-version"))
            .unwrap_err();
        assert!(matches!(error, AppError::PromotionConflict));
        assert_eq!(fixture.shot().generated_video_asset_version_id, None);
        assert_eq!(
            audit_payloads(&fixture.root, "shot.video.promoted").len(),
            0
        );
    }

    #[test]
    fn wrong_shot_or_operation_is_not_promotable() {
        let fixture = completed_shot_i2v_fixture();
        let other_shot = CinemaService::create_shot(
            &fixture.root,
            &fixture.scene_id,
            None,
            2.0,
            "Close",
            None,
            None,
        )
        .unwrap();
        let wrong_shot_artifact =
            fixture.capture_wrong_shot_artifact("attempt-wrong-shot", &other_shot.id);
        let error = fixture
            .promote_with(&wrong_shot_artifact, None)
            .unwrap_err();
        assert!(matches!(error, AppError::GenerationArtifactNotPromotable));

        let image_artifact = fixture.capture_wrong_operation_image_artifact("attempt-other-op");
        let error = fixture.promote_with(&image_artifact, None).unwrap_err();
        assert!(matches!(error, AppError::GenerationArtifactNotPromotable));

        assert_eq!(fixture.shot().generated_video_asset_version_id, None);
    }

    #[test]
    fn missing_lineage_is_rejected() {
        let fixture = completed_shot_i2v_fixture();
        let conn = db::open_existing_connection(&fixture.root.join("project.db")).unwrap();
        conn.execute(
            "DELETE FROM artifact_lineage WHERE artifact_id = ?1",
            [&fixture.artifact_id],
        )
        .unwrap();
        let error = fixture.promote(None).unwrap_err();
        assert!(matches!(error, AppError::GenerationLineageIncomplete));
    }

    #[test]
    fn non_video_artifact_is_rejected() {
        let fixture = completed_shot_i2v_fixture();
        let run_id = "run-image";
        fixture.insert_run(
            run_id,
            "shot.image_to_video",
            &format!(
                "{{\"sceneId\":\"{}\",\"shotId\":\"{}\",\"sourceAssetVersionId\":\"{}\"}}",
                fixture.scene_id, fixture.shot_id, fixture.source_version_id
            ),
        );
        let image_artifact = fixture.capture("attempt-image", run_id, "image", "c".repeat(64));
        let error = fixture.promote_with(&image_artifact, None).unwrap_err();
        assert!(matches!(error, AppError::GenerationArtifactNotPromotable));
    }

    #[test]
    fn promotion_uses_frozen_run_source_not_drifted_pin() {
        let fixture = completed_shot_i2v_fixture();
        // Re-pin a different keyframe after the run: the frozen run source
        // stays authoritative, so promotion still succeeds and the drifted
        // pin is untouched.
        let replacement_source = fixture.root.join("keyframe-2.png");
        let image: image::RgbaImage =
            image::ImageBuffer::from_pixel(8, 8, image::Rgba([200, 100, 50, 255]));
        image.save(&replacement_source).unwrap();
        let replacement_version = AssetService::import_asset_version(
            &fixture.root,
            &keyframe_asset_id(&fixture),
            &replacement_source,
            None,
        )
        .unwrap();
        AssetService::promote_asset_version(&fixture.root, &replacement_version.id).unwrap();
        CinemaService::set_shot_keyframe(
            &fixture.root,
            &fixture.shot_id,
            Some(&replacement_version.id),
        )
        .unwrap();

        let promoted = fixture.promote(None).unwrap();
        let shot = fixture.shot();
        assert_eq!(
            shot.generated_video_asset_version_id.as_deref(),
            Some(promoted.asset_version_id.as_str())
        );
        assert_eq!(
            shot.keyframe_asset_version_id.as_deref(),
            Some(replacement_version.id.as_str())
        );
    }

    fn keyframe_asset_id(fixture: &CompletedShot) -> String {
        let conn = db::open_existing_connection(&fixture.root.join("project.db")).unwrap();
        conn.query_row(
            "SELECT id FROM assets WHERE project_id = ?1 AND type = 'shot_keyframe' LIMIT 1",
            [fixture.project_id()],
            |row| row.get(0),
        )
        .unwrap()
    }
}
