#![forbid(unsafe_code)]
#![deny(clippy::all, clippy::pedantic)]

//! Independent fixture-result validation for the benchmark corpus. This crate
//! intentionally has no dependency on fixture generation or editor crates.

use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Oracle lock format version.
pub const ORACLE_LOCK_VERSION: u32 = 1;

/// A normalized non-negative timestamp used by independently decoded evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Rational {
    /// Numerator.
    pub numerator: u64,
    /// Denominator, always non-zero.
    pub denominator: u64,
}

impl Rational {
    /// Builds a normalized timestamp.
    ///
    /// # Panics
    ///
    /// Panics when `denominator` is zero.
    #[must_use]
    pub fn new(numerator: u64, denominator: u64) -> Self {
        assert_ne!(denominator, 0, "rational denominator must not be zero");
        let divisor = gcd(numerator, denominator);
        Self {
            numerator: numerator / divisor,
            denominator: denominator / divisor,
        }
    }

    /// Adds two values without silently overflowing.
    #[must_use]
    pub fn checked_add(self, other: Self) -> Option<Self> {
        let divisor = gcd(self.denominator, other.denominator);
        let left_factor = other.denominator / divisor;
        let right_factor = self.denominator / divisor;
        let left = self.numerator.checked_mul(left_factor)?;
        let right = other.numerator.checked_mul(right_factor)?;
        Some(Self::new(
            left.checked_add(right)?,
            self.denominator.checked_mul(left_factor)?,
        ))
    }
}

impl Ord for Rational {
    fn cmp(&self, other: &Self) -> Ordering {
        let left = u128::from(self.numerator) * u128::from(other.denominator);
        let right = u128::from(other.numerator) * u128::from(self.denominator);
        left.cmp(&right)
    }
}

impl PartialOrd for Rational {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn gcd(mut left: u64, mut right: u64) -> u64 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left.max(1)
}

/// A decoded frame observation from an independent demux/decode backend.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FrameEvidence {
    /// Source index after decode ordering.
    pub index: u64,
    /// Exact decoded PTS.
    pub pts: Rational,
    /// Exact decoded duration.
    pub duration: Rational,
    /// Semantic frame digest from the independent CPU reference.
    pub semantic_sha256: String,
}

/// A bounded decoded audio observation from an independent audio reference.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AudioEvidence {
    /// First 48 kHz sample index in this block.
    pub start_sample: u64,
    /// Stereo sample frames in this block.
    pub sample_frames: u32,
    /// Semantic digest of the block after fades and resampling.
    pub semantic_sha256: String,
}

/// Metadata reported by an independent container parser.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ContainerEvidence {
    /// Codec configuration/sample-entry identifier.
    pub sample_entry: String,
    /// Media timescale.
    pub timescale: u32,
    /// Declared encoder delay.
    pub encoder_delay_samples: u32,
    /// Declared encoder padding.
    pub encoder_padding_samples: u32,
    /// Actual decoder path recorded by the adapter.
    pub codec_path: String,
}

/// A named A/V synchronization marker measured from decoded output.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MarkerEvidence {
    /// Stable marker name.
    pub name: String,
    /// Decoded location.
    pub pts: Rational,
}

/// All independent observations for one generated artifact.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DecodedEvidence {
    /// Fixture identifier.
    pub fixture_id: String,
    /// Generator epoch embedded in the artifact manifest.
    pub generation: String,
    /// Artifact SHA-256 supplied by the generation run.
    pub artifact_sha256: String,
    /// Rec.709 color intent reported by the independent metadata checker.
    pub color_metadata: String,
    /// Frames in presentation order.
    pub frames: Vec<FrameEvidence>,
    /// Audio blocks in sample order.
    pub audio: Vec<AudioEvidence>,
    /// Measured sync markers.
    pub markers: Vec<MarkerEvidence>,
    /// Container metadata.
    pub container: ContainerEvidence,
}

/// Expected value and mandatory decoded truth for a fixture run.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OracleExpectation {
    /// Fixture identity.
    pub fixture_id: String,
    /// The generation epoch this verification accepts.
    pub generation: String,
    /// Expected encoded artifact digest.
    pub artifact_sha256: String,
    /// Required Rec.709 metadata declaration.
    pub color_metadata: String,
    /// Expected decoded frames.
    pub frames: Vec<FrameEvidence>,
    /// Expected decoded audio blocks.
    pub audio: Vec<AudioEvidence>,
    /// Expected A/V markers.
    pub markers: Vec<MarkerEvidence>,
    /// Expected container fields.
    pub container: ContainerEvidence,
}

/// One deterministic oracle failure. A verifier returns all failures so a
/// benchmark disclosure preserves the complete diagnosis.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Violation {
    /// Machine-readable failure code.
    pub code: String,
    /// Specific observation which failed.
    pub detail: String,
}

/// A locked component description. Each component must be independently
/// versioned and hashed before any public benchmark claim.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LockedComponent {
    /// Component role.
    pub role: String,
    /// Version string.
    pub version: String,
    /// Repository-relative checked-in source or contract path.
    pub source_path: String,
    /// Stable source/contract identity declared by that path.
    pub spec_identity: String,
    /// Source/artifact SHA-256.
    pub sha256: String,
}

/// Checked-in lock data for the independent generator/reference/verifier set.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OracleLock {
    /// Lock schema version.
    pub schema_version: u32,
    /// Components which must remain pinned.
    pub components: Vec<LockedComponent>,
    /// Numerical tolerance names and values.
    pub tolerances: serde_json::Value,
}

impl OracleLock {
    /// Loads the checked-in independent verifier lock.
    ///
    /// # Errors
    ///
    /// Returns an error when the checked-in JSON does not conform to the lock
    /// schema or does not declare every required independent component.
    pub fn load_checked_in() -> Result<Self, String> {
        let lock: Self = serde_json::from_str(include_str!("../oracle-lock.json"))
            .map_err(|error| error.to_string())?;
        if lock.schema_version != ORACLE_LOCK_VERSION {
            return Err("unsupported oracle lock schema".to_owned());
        }
        let roles: BTreeSet<_> = lock
            .components
            .iter()
            .map(|component| component.role.as_str())
            .collect();
        let required = [
            "fixture-generator",
            "cpu-reference",
            "oracle-verifier",
            "independent-verifier-contract",
            "metadata-checker",
            "tolerance-set",
        ];
        if required.iter().any(|role| !roles.contains(role)) {
            return Err("oracle lock omits a required independent component".to_owned());
        }
        if lock
            .components
            .iter()
            .any(|component| !is_sha256(&component.sha256))
        {
            return Err("oracle lock has an invalid component SHA-256".to_owned());
        }
        lock.verify_integrity()?;
        Ok(lock)
    }

    /// Re-hashes every checked-in lock source and rejects path traversal or a
    /// stale component hash.
    ///
    /// # Errors
    ///
    /// Returns an explanation if a pinned path is unsafe, missing, unreadable,
    /// or no longer matches the SHA-256 recorded in the lock.
    pub fn verify_integrity(&self) -> Result<(), String> {
        let repository_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .map_err(|error| format!("cannot resolve repository root: {error}"))?;
        for component in &self.components {
            let relative = Path::new(&component.source_path);
            if relative.is_absolute()
                || relative
                    .components()
                    .any(|component| matches!(component, std::path::Component::ParentDir))
            {
                return Err(format!(
                    "unsafe oracle lock source path: {}",
                    component.source_path
                ));
            }
            if component.spec_identity.is_empty() {
                return Err(format!("missing spec identity for {}", component.role));
            }
            let source_path = repository_root.join(relative);
            let bytes = fs::read(&source_path)
                .map_err(|error| format!("cannot read {}: {error}", component.source_path))?;
            if sha256_hex(&bytes) != component.sha256 {
                return Err(format!(
                    "oracle lock hash mismatch: {}",
                    component.source_path
                ));
            }
        }
        Ok(())
    }
}

/// Verifies independently decoded evidence against a checked source oracle.
#[must_use]
pub fn verify(expected: &OracleExpectation, observed: &DecodedEvidence) -> Vec<Violation> {
    let mut violations = Vec::new();
    compare_scalar(
        &mut violations,
        "fixture-id",
        &expected.fixture_id,
        &observed.fixture_id,
    );
    compare_scalar(
        &mut violations,
        "stale-generation",
        &expected.generation,
        &observed.generation,
    );
    compare_scalar(
        &mut violations,
        "artifact-digest",
        &expected.artifact_sha256,
        &observed.artifact_sha256,
    );
    compare_scalar(
        &mut violations,
        "color-metadata",
        &expected.color_metadata,
        &observed.color_metadata,
    );
    compare_container(&mut violations, &expected.container, &observed.container);
    compare_frames(&mut violations, &expected.frames, &observed.frames);
    compare_audio(&mut violations, &expected.audio, &observed.audio);
    compare_markers(&mut violations, &expected.markers, &observed.markers);
    violations
}

fn compare_scalar(violations: &mut Vec<Violation>, code: &str, expected: &str, observed: &str) {
    if expected != observed {
        violations.push(Violation {
            code: code.to_owned(),
            detail: format!("expected {expected:?}, observed {observed:?}"),
        });
    }
}

fn compare_container(
    violations: &mut Vec<Violation>,
    expected: &ContainerEvidence,
    observed: &ContainerEvidence,
) {
    if expected != observed {
        violations.push(Violation {
            code: "container-metadata".to_owned(),
            detail: "sample entry, timescale, delay/padding, or codec path differs".to_owned(),
        });
    }
}

fn compare_frames(
    violations: &mut Vec<Violation>,
    expected: &[FrameEvidence],
    observed: &[FrameEvidence],
) {
    if expected.len() != observed.len() {
        violations.push(Violation {
            code: "frame-count".to_owned(),
            detail: format!("expected {}, observed {}", expected.len(), observed.len()),
        });
    }
    let mut previous_end = None;
    for (index, frame) in observed.iter().enumerate() {
        if let Some(end) = previous_end
            && frame.pts < end
        {
            violations.push(Violation {
                code: "non-monotonic-pts".to_owned(),
                detail: format!("frame {index} starts before the previous frame ends"),
            });
        }
        previous_end = frame.pts.checked_add(frame.duration);
        match expected.get(index) {
            Some(expected_frame) if expected_frame == frame => {}
            Some(expected_frame) => {
                let code = if expected_frame.semantic_sha256 == frame.semantic_sha256 {
                    "frame-time-map"
                } else {
                    "frame-semantic-digest"
                };
                violations.push(Violation {
                    code: code.to_owned(),
                    detail: format!("frame {index} differs from independent source truth"),
                });
            }
            None => break,
        }
    }
}

fn compare_audio(
    violations: &mut Vec<Violation>,
    expected: &[AudioEvidence],
    observed: &[AudioEvidence],
) {
    if expected.len() != observed.len() {
        violations.push(Violation {
            code: "audio-block-count".to_owned(),
            detail: format!("expected {}, observed {}", expected.len(), observed.len()),
        });
    }
    let mut next_sample = 0_u64;
    for (index, block) in observed.iter().enumerate() {
        if block.start_sample != next_sample {
            violations.push(Violation {
                code: "audio-sample-count".to_owned(),
                detail: format!("audio block {index} has a discontinuous sample range"),
            });
        }
        next_sample = block.start_sample + u64::from(block.sample_frames);
        match expected.get(index) {
            Some(expected_block) if expected_block == block => {}
            Some(expected_block) => {
                let code = if expected_block.semantic_sha256 == block.semantic_sha256 {
                    "audio-sample-count"
                } else {
                    "audio-fade-or-digest"
                };
                violations.push(Violation {
                    code: code.to_owned(),
                    detail: format!("audio block {index} differs from independent source truth"),
                });
            }
            None => break,
        }
    }
}

fn compare_markers(
    violations: &mut Vec<Violation>,
    expected: &[MarkerEvidence],
    observed: &[MarkerEvidence],
) {
    if expected != observed {
        violations.push(Violation {
            code: "sync-markers".to_owned(),
            detail: "decoded flash/chirp/impulse markers differ from the recipe".to_owned(),
        });
    }
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// SHA-256 helper for independently authored oracle components and reports.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(&mut output, "{byte:02x}").expect("writing into String cannot fail");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::{
        AudioEvidence, ContainerEvidence, DecodedEvidence, FrameEvidence, MarkerEvidence,
        OracleExpectation, OracleLock, Rational, verify,
    };

    fn expected() -> OracleExpectation {
        OracleExpectation {
            fixture_id: "QHD-I".to_owned(),
            generation: "generation-2026-07-18-a".to_owned(),
            artifact_sha256: "a".repeat(64),
            color_metadata: "Rec.709".to_owned(),
            frames: vec![
                FrameEvidence {
                    index: 0,
                    pts: Rational::new(0, 1),
                    duration: Rational::new(1, 30),
                    semantic_sha256: "b".repeat(64),
                },
                FrameEvidence {
                    index: 1,
                    pts: Rational::new(1, 30),
                    duration: Rational::new(1, 30),
                    semantic_sha256: "c".repeat(64),
                },
            ],
            audio: vec![AudioEvidence {
                start_sample: 0,
                sample_frames: 1_024,
                semantic_sha256: "d".repeat(64),
            }],
            markers: vec![MarkerEvidence {
                name: "flash-chirp-01".to_owned(),
                pts: Rational::new(1, 1),
            }],
            container: ContainerEvidence {
                sample_entry: "avc1 + mp4a".to_owned(),
                timescale: 90_000,
                encoder_delay_samples: 0,
                encoder_padding_samples: 0,
                codec_path: "software-avc".to_owned(),
            },
        }
    }

    fn observed() -> DecodedEvidence {
        let expected = expected();
        DecodedEvidence {
            fixture_id: expected.fixture_id,
            generation: expected.generation,
            artifact_sha256: expected.artifact_sha256,
            color_metadata: expected.color_metadata,
            frames: expected.frames,
            audio: expected.audio,
            markers: expected.markers,
            container: expected.container,
        }
    }

    fn codes(observed: &DecodedEvidence) -> Vec<String> {
        verify(&expected(), observed)
            .into_iter()
            .map(|violation| violation.code)
            .collect()
    }

    #[test]
    fn checked_in_lock_pins_all_independent_components() {
        let lock = OracleLock::load_checked_in().expect("valid checked-in oracle lock");
        assert_eq!(lock.components.len(), 6);
        assert!(lock.verify_integrity().is_ok());
    }

    #[test]
    fn valid_independent_evidence_passes() {
        assert!(codes(&observed()).is_empty());
    }

    #[test]
    fn one_frame_and_pts_faults_fail() {
        let mut evidence = observed();
        evidence.frames[0].semantic_sha256 = "e".repeat(64);
        evidence.frames[1].pts = Rational::new(0, 1);
        let result = codes(&evidence);
        assert!(result.iter().any(|code| code == "frame-semantic-digest"));
        assert!(result.iter().any(|code| code == "non-monotonic-pts"));
    }

    #[test]
    fn one_sample_and_audio_fade_faults_fail() {
        let mut evidence = observed();
        evidence.audio[0].sample_frames = 1_023;
        assert!(
            codes(&evidence)
                .iter()
                .any(|code| code == "audio-sample-count")
        );

        let mut evidence = observed();
        evidence.audio[0].semantic_sha256 = "e".repeat(64);
        assert!(
            codes(&evidence)
                .iter()
                .any(|code| code == "audio-fade-or-digest")
        );
    }

    #[test]
    fn color_container_marker_and_stale_generation_faults_fail() {
        let mut evidence = observed();
        evidence.generation = "generation-before-current".to_owned();
        evidence.color_metadata = "Rec.601".to_owned();
        evidence.container.timescale = 1_000;
        evidence.markers[0].pts = Rational::new(2, 1);
        let result = codes(&evidence);
        for expected_code in [
            "stale-generation",
            "color-metadata",
            "container-metadata",
            "sync-markers",
        ] {
            assert!(result.iter().any(|code| code == expected_code));
        }
    }
}
