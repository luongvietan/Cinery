//! P10.3 production-evidence spike.
//!
//! It proves which evidence modes the production Tauri library can prepare
//! without consulting `PATH` for a media decoder.

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use cinematic_desktop_lib::video_qa::evidence::{
    prepare_packaged_evidence, DirectVideoEvidence, EvidenceMode, EvidencePathError,
    PreparedEvidence, TemporalDecoderAvailability, VIDEO_QA_EVIDENCE_UNSUPPORTED,
};
use std::{fs, path::Path};

fn write_fixture_video(path: &Path) {
    // Decodable H.264/AVC fixture from Web Platform Tests:
    // media-source/mp4/test-v-128k-640x480-30fps-10kfr.mp4
    // Declared there as video/mp4; codecs="avc1.4D4001" (BSD-3-Clause).
    let encoded: String = include_str!("fixtures/video_qa_wpt_h264.mp4.b64")
        .split_whitespace()
        .collect();
    let fixture = BASE64_STANDARD.decode(encoded).unwrap();

    // Guard against replacing the checked-in playable fixture with another
    // signature-only stub: this file has movie/track metadata, AVC decoder
    // configuration, fragmented samples, and media payload.
    for marker in [b"moov", b"trak", b"avc1", b"avcC", b"moof", b"mdat"] {
        assert!(
            fixture.windows(marker.len()).any(|window| window == marker),
            "fixture must contain the {marker:?} box/entry"
        );
    }
    fs::write(path, fixture).unwrap();
}

#[test]
fn direct_video_mode_is_deterministic_without_a_path_decoder() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("candidate.mp4");
    write_fixture_video(&source);

    let first = prepare_packaged_evidence(
        &source,
        EvidenceMode::DirectVideo,
        TemporalDecoderAvailability::Unavailable,
    )
    .unwrap();
    let second = prepare_packaged_evidence(
        &source,
        EvidenceMode::DirectVideo,
        TemporalDecoderAvailability::Unavailable,
    )
    .unwrap();

    assert_eq!(first, second);
    assert_eq!(
        first,
        PreparedEvidence::DirectVideo(DirectVideoEvidence {
            source_content_sha256:
                "1743855560ef42b195a58901fc634881ad1dd6b01394ce8feedd23cfb25a3fbf".into(),
            mime_type: "video/mp4",
            size_bytes: 27_764,
        })
    );
}

#[test]
fn sampled_frame_mode_returns_typed_unsupported_when_decoder_is_explicitly_unavailable() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("candidate.mp4");
    write_fixture_video(&source);

    let result = prepare_packaged_evidence(
        &source,
        EvidenceMode::SampledFrames,
        TemporalDecoderAvailability::Unavailable,
    );

    assert_eq!(
        result,
        Err(EvidencePathError::EvidenceUnsupported {
            code: VIDEO_QA_EVIDENCE_UNSUPPORTED,
            mode: EvidenceMode::SampledFrames,
            reason: "sampled-frame evidence requires an explicitly configured decoder bundled with the Tauri application",
        })
    );
}
