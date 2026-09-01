//! Package-compatible evidence preparation for Video QA adapters.

use sha2::{Digest, Sha256};
use std::{fs, io, path::Path};

pub const VIDEO_QA_EVIDENCE_UNSUPPORTED: &str = "VIDEO_QA_EVIDENCE_UNSUPPORTED";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvidenceMode {
    DirectVideo,
    SampledFrames,
}

/// Decoder availability supplied by packaging/runtime configuration.
///
/// This value is explicit; evidence preparation never probes the host or
/// searches `PATH` for a decoder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TemporalDecoderAvailability {
    Unavailable,
}

#[derive(Debug, PartialEq, Eq)]
pub struct DirectVideoEvidence {
    pub source_content_sha256: String,
    pub mime_type: &'static str,
    pub size_bytes: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PreparedEvidence {
    DirectVideo(DirectVideoEvidence),
}

#[derive(Debug, PartialEq, Eq)]
pub enum EvidencePathError {
    Unreadable(io::ErrorKind),
    InvalidMp4,
    EvidenceUnsupported {
        code: &'static str,
        mode: EvidenceMode,
        reason: &'static str,
    },
}

pub fn prepare_packaged_evidence(
    source: &Path,
    mode: EvidenceMode,
    decoder: TemporalDecoderAvailability,
) -> Result<PreparedEvidence, EvidencePathError> {
    let bytes = fs::read(source).map_err(|error| EvidencePathError::Unreadable(error.kind()))?;
    // This is intentionally the application's existing minimal MP4 signature
    // check, not a claim that the container or its media samples were decoded.
    if !crate::generation::storage::looks_like_mp4(&bytes) {
        return Err(EvidencePathError::InvalidMp4);
    }

    match mode {
        EvidenceMode::DirectVideo => Ok(PreparedEvidence::DirectVideo(DirectVideoEvidence {
            source_content_sha256: format!("{:x}", Sha256::digest(&bytes)),
            mime_type: "video/mp4",
            size_bytes: bytes.len() as u64,
        })),
        EvidenceMode::SampledFrames => match decoder {
            TemporalDecoderAvailability::Unavailable => {
                Err(EvidencePathError::EvidenceUnsupported {
                    code: VIDEO_QA_EVIDENCE_UNSUPPORTED,
                    mode,
                    reason: "sampled-frame evidence requires an explicitly configured decoder bundled with the Tauri application",
                })
            }
        },
    }
}
