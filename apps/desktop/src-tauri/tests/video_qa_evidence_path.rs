//! P10.3 production-evidence spike.
//!
//! This test deliberately keeps the boundary independent of the Video QA
//! domain. It proves which evidence modes can be prepared from a packaged
//! desktop application without consulting `PATH` for a media decoder.

use sha2::{Digest, Sha256};
use std::{fs, io, path::Path};

const VIDEO_QA_EVIDENCE_UNSUPPORTED: &str = "VIDEO_QA_EVIDENCE_UNSUPPORTED";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EvidenceMode {
    DirectVideo,
    SampledFrames,
}

#[derive(Debug, PartialEq, Eq)]
enum PackagedDecoderBinding {
    NotBundled,
}

#[derive(Debug, PartialEq, Eq)]
struct DirectVideoEvidence {
    source_content_sha256: String,
    mime_type: &'static str,
    size_bytes: u64,
}

#[derive(Debug, PartialEq, Eq)]
enum PreparedEvidence {
    DirectVideo(DirectVideoEvidence),
}

#[derive(Debug, PartialEq, Eq)]
enum EvidencePathError {
    Unreadable(io::ErrorKind),
    InvalidMp4,
    EvidenceUnsupported {
        code: &'static str,
        mode: EvidenceMode,
        reason: &'static str,
    },
}

fn prepare_packaged_evidence(
    source: &Path,
    mode: EvidenceMode,
    decoder: PackagedDecoderBinding,
) -> Result<PreparedEvidence, EvidencePathError> {
    let bytes = fs::read(source).map_err(|error| EvidencePathError::Unreadable(error.kind()))?;
    if !cinematic_desktop_lib::generation::storage::looks_like_mp4(&bytes) {
        return Err(EvidencePathError::InvalidMp4);
    }

    match mode {
        EvidenceMode::DirectVideo => Ok(PreparedEvidence::DirectVideo(DirectVideoEvidence {
            source_content_sha256: format!("{:x}", Sha256::digest(&bytes)),
            mime_type: "video/mp4",
            size_bytes: bytes.len() as u64,
        })),
        EvidenceMode::SampledFrames => match decoder {
            PackagedDecoderBinding::NotBundled => Err(EvidencePathError::EvidenceUnsupported {
                code: VIDEO_QA_EVIDENCE_UNSUPPORTED,
                mode,
                reason:
                    "sampled-frame evidence requires a decoder bundled with the Tauri application",
            }),
        },
    }
}

fn write_fixture_video(path: &Path) {
    // A deterministic ISO-BMFF fixture: a valid `ftyp` box followed by an
    // empty `mdat` box. Direct-video evidence needs exact readable bytes;
    // sampled-frame evidence additionally needs a packaged decoder.
    const FIXTURE: &[u8] = &[
        0x00, 0x00, 0x00, 0x18, b'f', b't', b'y', b'p', b'i', b's', b'o', b'm', 0x00, 0x00, 0x02,
        0x00, b'i', b's', b'o', b'm', b'i', b's', b'o', b'2', 0x00, 0x00, 0x00, 0x08, b'm', b'd',
        b'a', b't',
    ];
    fs::write(path, FIXTURE).unwrap();
}

#[test]
fn direct_video_mode_is_deterministic_without_a_path_decoder() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("candidate.mp4");
    write_fixture_video(&source);

    let first = prepare_packaged_evidence(
        &source,
        EvidenceMode::DirectVideo,
        PackagedDecoderBinding::NotBundled,
    )
    .unwrap();
    let second = prepare_packaged_evidence(
        &source,
        EvidenceMode::DirectVideo,
        PackagedDecoderBinding::NotBundled,
    )
    .unwrap();

    assert_eq!(first, second);
    assert_eq!(
        first,
        PreparedEvidence::DirectVideo(DirectVideoEvidence {
            source_content_sha256:
                "de17900c6e6225de65eec7b1970c03636113a98dc8a9c0e9ea5e937fa7ad32f8".into(),
            mime_type: "video/mp4",
            size_bytes: 32,
        })
    );
}

#[test]
fn sampled_frame_mode_returns_typed_unsupported_without_a_bundled_decoder() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("candidate.mp4");
    write_fixture_video(&source);

    let result = prepare_packaged_evidence(
        &source,
        EvidenceMode::SampledFrames,
        PackagedDecoderBinding::NotBundled,
    );

    assert_eq!(
        result,
        Err(EvidencePathError::EvidenceUnsupported {
            code: VIDEO_QA_EVIDENCE_UNSUPPORTED,
            mode: EvidenceMode::SampledFrames,
            reason: "sampled-frame evidence requires a decoder bundled with the Tauri application",
        })
    );
}
