#![forbid(unsafe_code)]
#![deny(clippy::all, clippy::pedantic)]

//! Deterministic source recipes and streaming truth generators for the editor
//! benchmark corpus. Encoded media is deliberately generated outside git.

use std::cmp::Ordering;
use std::fmt::Write as _;
use std::io::{Seek, SeekFrom, Write};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The format contract shared by all mandatory fixture recipes.
pub const BASELINE_AUDIO: &str = "AAC-LC stereo, 48 kHz, 192 kb/s";
/// Source generator identity, pinned in every fixture manifest.
pub const GENERATOR_ID: &str = "stitch-fixtures-rust-v1";
/// The fixed sample rate of the lossless audio oracle.
pub const SAMPLE_RATE: u32 = 48_000;

/// A normalized, non-negative rational timestamp.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Rational {
    /// Numerator.
    pub numerator: u64,
    /// Denominator, always non-zero.
    pub denominator: u64,
}

impl Rational {
    /// Creates a normalized rational value.
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

    /// Adds two non-negative rational values without silently overflowing.
    #[must_use]
    pub fn checked_add(self, other: Self) -> Option<Self> {
        let common_divisor = gcd(self.denominator, other.denominator);
        let left_factor = other.denominator / common_divisor;
        let right_factor = self.denominator / common_divisor;
        let left = self.numerator.checked_mul(left_factor)?;
        let right = other.numerator.checked_mul(right_factor)?;
        let numerator = left.checked_add(right)?;
        let denominator = self.denominator.checked_mul(left_factor)?;
        Some(Self::new(numerator, denominator))
    }

    /// Returns a stable `numerator/denominator` representation.
    #[must_use]
    pub fn canonical(self) -> String {
        format!("{}/{}", self.numerator, self.denominator)
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

fn difference(left: Rational, right: Rational) -> Rational {
    debug_assert!(left >= right);
    let common_divisor = gcd(left.denominator, right.denominator);
    let left_factor = right.denominator / common_divisor;
    let right_factor = left.denominator / common_divisor;
    let numerator = left
        .numerator
        .checked_mul(left_factor)
        .expect("fixture timestamp subtraction cannot overflow")
        - right
            .numerator
            .checked_mul(right_factor)
            .expect("fixture timestamp subtraction cannot overflow");
    Rational::new(
        numerator,
        left.denominator
            .checked_mul(left_factor)
            .expect("fixture timestamp subtraction cannot overflow"),
    )
}

/// Fixture identifiers covered by the first vertical slice.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FixtureId {
    /// 2560×1440, 30 fps, all-IDR AVC.
    QhdI,
    /// 2560×1440, 60 fps, long-GOP AVC.
    QhdLgop,
    /// 3072×1728, 30 fps, all-IDR AVC.
    ThreeKI,
    /// Optional capability fixture, not a release performance gate.
    ThreeKLgop,
    /// Alternating exact durations with audio/video sync markers.
    VfrAv,
}

impl FixtureId {
    /// Stable public fixture identifier.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::QhdI => "QHD-I",
            Self::QhdLgop => "QHD-LGOP",
            Self::ThreeKI => "3K-I",
            Self::ThreeKLgop => "3K-LGOP",
            Self::VfrAv => "VFR-AV",
        }
    }
}

/// Development and release corpus durations.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CorpusDuration {
    /// 30 seconds for fast deterministic iteration.
    Development,
    /// 600 seconds for release evidence.
    Release,
}

impl CorpusDuration {
    /// Exact duration in seconds.
    #[must_use]
    pub const fn seconds(self) -> u64 {
        match self {
            Self::Development => 30,
            Self::Release => 600,
        }
    }
}

/// A marker that must survive import, edit, export, and independent decoding.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Marker {
    /// Stable marker name.
    pub name: String,
    /// Exact source timeline location.
    pub pts: Rational,
    /// Marker kind, such as `flash`, `chirp`, or `single-sample-impulse`.
    pub kind: String,
}

/// Fixed codec/container metadata which platform encoders must disclose.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CodecContract {
    /// Video codec name.
    pub video_codec: String,
    /// AVC/HEVC/AV1 profile.
    pub video_profile: String,
    /// Codec level, if applicable.
    pub video_level: String,
    /// Eight-bit 4:2:0 for the mandatory AVC baseline.
    pub chroma_format: String,
    /// Target video bitrate in bits per second.
    pub video_bitrate_bps: u64,
    /// Closed GOP and random-access policy.
    pub gop: String,
    /// Audio codec and required baseline.
    pub audio_codec: String,
    /// Target audio bitrate in bits per second.
    pub audio_bitrate_bps: u32,
    /// MP4 sample entry identifier.
    pub sample_entry: String,
    /// MP4 media timescale.
    pub timescale: u32,
    /// Declared AAC encoder delay, before a platform encoder fills it in.
    pub encoder_delay_samples: u32,
    /// Declared AAC end padding, before a platform encoder fills it in.
    pub encoder_padding_samples: u32,
    /// Encoder implementation selected at generation time.
    pub encoder_identity: String,
}

/// The only two AAC bitrate classes accepted by this corpus.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AudioBitrateTier {
    /// Cross-platform mandatory baseline. It must be exactly 192 kb/s.
    MandatoryBaseline,
    /// Explicit capability probe. It must be exactly 320 kb/s and is never a
    /// mandatory fixture or release gate.
    OptionalCapability,
}

/// Requested or observed codec/container fields captured in an artifact record.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CodecProperties {
    /// Video codec name.
    pub video_codec: String,
    /// Video profile.
    pub video_profile: String,
    /// Video level.
    pub video_level: String,
    /// Target or actual video bitrate.
    pub video_bitrate_bps: u64,
    /// GOP/random-access declaration.
    pub gop: String,
    /// Audio codec name.
    pub audio_codec: String,
    /// Target or actual audio bitrate.
    pub audio_bitrate_bps: u32,
    /// Container sample entry.
    pub sample_entry: String,
    /// Hash of codec configuration bytes.
    pub configuration_sha256: String,
    /// Video media timescale.
    pub video_timescale: u32,
    /// Audio media timescale.
    pub audio_timescale: u32,
    /// Encoder delay in audio samples.
    pub encoder_delay_samples: u32,
    /// Encoder padding in audio samples.
    pub encoder_padding_samples: u32,
}

/// A typed codec request which records whether 192 or 320 kb/s was intended.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CodecRequest {
    /// Requested stream/container properties.
    pub properties: CodecProperties,
    /// Mandatory baseline versus optional capability intent.
    pub audio_bitrate_tier: AudioBitrateTier,
}

impl CodecRequest {
    /// Validates the non-negotiable AAC bitrate policy.
    ///
    /// # Errors
    ///
    /// Returns an explanation when the requested codec or bitrate does not
    /// match its declared tier.
    pub fn validate(&self) -> Result<(), String> {
        match self.audio_bitrate_tier {
            AudioBitrateTier::MandatoryBaseline
                if self.properties.audio_codec == "AAC-LC"
                    && self.properties.audio_bitrate_bps == 192_000 =>
            {
                Ok(())
            }
            AudioBitrateTier::MandatoryBaseline => {
                Err("mandatory baseline must be AAC-LC at exactly 192000 bps".to_owned())
            }
            AudioBitrateTier::OptionalCapability
                if self.properties.audio_bitrate_bps == 320_000 =>
            {
                Ok(())
            }
            AudioBitrateTier::OptionalCapability => {
                Err("optional capability tier must request exactly 320000 bps".to_owned())
            }
        }
    }
}

/// Capability-probe outcome. Unsupported optional media remains explicit and
/// must never silently fall back to an undisclosed codec path.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CapabilityResult {
    /// Schema version.
    pub schema_version: u32,
    /// Fixture identity.
    pub fixture_id: String,
    /// Requested codec/container properties and bitrate tier.
    pub requested: CodecRequest,
    /// Whether the requested capability was available.
    pub supported: bool,
    /// Actual properties selected by the platform, if supported.
    pub actual: Option<CodecProperties>,
    /// Platform encoder identity, such as a `VideoToolbox` or `Media Foundation` implementation.
    pub platform_encoder_identity: String,
    /// Typed reason for an unsupported capability or a mismatch.
    pub diagnostic: Option<String>,
}

impl CapabilityResult {
    /// Validates a capability disclosure before it is published.
    ///
    /// # Errors
    ///
    /// Returns an explanation when the request, support state, or platform
    /// identity is incomplete or internally inconsistent.
    pub fn validate(&self) -> Result<(), String> {
        self.requested.validate()?;
        if self.supported && self.actual.is_none() {
            return Err(
                "supported capability result must contain actual codec properties".to_owned(),
            );
        }
        if !self.supported && self.actual.is_some() {
            return Err(
                "unsupported capability result must not contain actual codec properties".to_owned(),
            );
        }
        if self.platform_encoder_identity.is_empty() {
            return Err("platform encoder identity must be recorded".to_owned());
        }
        Ok(())
    }
}

/// Exact container sample ledger captured alongside a generated artifact.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SampleLedger {
    /// Decoded video frame count.
    pub video_frame_count: u64,
    /// Decoded audio sample-frame count per channel.
    pub audio_sample_frames: u64,
    /// SHA-256 ledger of video PTS/duration records.
    pub video_timing_sha256: String,
    /// SHA-256 ledger of audio sample-position records.
    pub audio_timing_sha256: String,
}

/// Full external record for an encoded artifact. It is intentionally a schema
/// only: encoded media is generated under controlled storage and is not in git.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EncodedArtifactManifest {
    /// Schema version.
    pub schema_version: u32,
    /// Fixture identity.
    pub fixture_id: String,
    /// Digest of the checked-in source recipe.
    pub source_recipe_sha256: String,
    /// Requested codec/container properties.
    pub requested: CodecRequest,
    /// Actual platform-selected codec/container properties.
    pub actual: CodecProperties,
    /// Exact decoded sample ledgers.
    pub sample_ledger: SampleLedger,
    /// SHA-256 of the external encoded artifact.
    pub artifact_sha256: String,
    /// Platform encoder identity.
    pub platform_encoder_identity: String,
    /// License or controlled-storage terms.
    pub license: String,
}

impl EncodedArtifactManifest {
    /// Validates mandatory audio and mandatory disclosure fields.
    ///
    /// # Errors
    ///
    /// Returns an explanation when the mandatory AAC policy or required
    /// digest/disclosure field is invalid.
    pub fn validate(&self) -> Result<(), String> {
        self.requested.validate()?;
        if self.requested.audio_bitrate_tier == AudioBitrateTier::MandatoryBaseline
            && (self.actual.audio_codec != "AAC-LC" || self.actual.audio_bitrate_bps != 192_000)
        {
            return Err(
                "mandatory artifact must actually encode AAC-LC at exactly 192000 bps".to_owned(),
            );
        }
        for (label, value) in [
            ("source recipe", self.source_recipe_sha256.as_str()),
            ("artifact", self.artifact_sha256.as_str()),
            (
                "video timing",
                self.sample_ledger.video_timing_sha256.as_str(),
            ),
            (
                "audio timing",
                self.sample_ledger.audio_timing_sha256.as_str(),
            ),
            (
                "codec configuration",
                self.actual.configuration_sha256.as_str(),
            ),
        ] {
            if !is_sha256(value) {
                return Err(format!("{label} SHA-256 must be 64 hexadecimal characters"));
            }
        }
        if self.platform_encoder_identity.is_empty() || self.license.is_empty() {
            return Err("platform encoder identity and license are required".to_owned());
        }
        Ok(())
    }
}

/// A complete deterministic fixture recipe. The recipe describes source truth;
/// generated MP4 files are external artifacts and must not be committed.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FixtureRecipe {
    /// Manifest schema version.
    pub schema_version: u32,
    /// Stable fixture name.
    pub fixture_id: String,
    /// Development/release duration selector.
    pub corpus_duration: CorpusDuration,
    /// Exact duration.
    pub duration: Rational,
    /// Source width.
    pub width: u32,
    /// Source height.
    pub height: u32,
    /// Pixel aspect ratio.
    pub pixel_aspect_ratio: Rational,
    /// Rec.709 color intent, transfer, and matrix.
    pub color_metadata: String,
    /// Constant frame rate where present.
    pub frame_rate: Option<Rational>,
    /// Alternating VFR durations where present.
    pub vfr_frame_durations: Vec<Rational>,
    /// Codec/container contract.
    pub codec: CodecContract,
    /// Deterministic A/V markers.
    pub markers: Vec<Marker>,
    /// Source generator identity.
    pub generator_identity: String,
    /// License declaration for recipe/source material.
    pub license: String,
    /// Semantic decoded stream digest slot populated by the generator.
    pub semantic_sha256: String,
    /// Encoded artifact digest slot, populated only for generated artifacts.
    pub artifact_sha256: Option<String>,
}

impl FixtureRecipe {
    /// Builds a source recipe for a fixture and corpus duration.
    #[must_use]
    pub fn new(fixture_id: FixtureId, corpus_duration: CorpusDuration) -> Self {
        let (width, height, frame_rate, vfr_frame_durations, codec) = fixture_contract(fixture_id);
        let duration = Rational::new(corpus_duration.seconds(), 1);
        let mut recipe = Self {
            schema_version: 1,
            fixture_id: fixture_id.as_str().to_owned(),
            corpus_duration,
            duration,
            width,
            height,
            pixel_aspect_ratio: Rational::new(1, 1),
            color_metadata: "Rec.709/sRGB/BT.709/full-range-RGB-source".to_owned(),
            frame_rate,
            vfr_frame_durations,
            codec,
            markers: deterministic_markers(duration),
            generator_identity: GENERATOR_ID.to_owned(),
            license: "CC0-1.0 source recipe and generated truth; encoded artifacts are external controlled outputs".to_owned(),
            semantic_sha256: String::new(),
            artifact_sha256: None,
        };
        recipe.semantic_sha256 = FixtureStream::new(recipe.clone()).semantic_digest();
        recipe
    }

    /// Creates a streaming generator without allocating decoded video or audio.
    #[must_use]
    pub fn stream(&self) -> FixtureStream {
        FixtureStream::new(self.clone())
    }

    /// Returns canonical, pretty JSON suitable for check-in as a recipe.
    ///
    /// # Errors
    ///
    /// Returns the serializer error if the recipe cannot be encoded as JSON.
    pub fn canonical_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

fn fixture_contract(
    fixture_id: FixtureId,
) -> (u32, u32, Option<Rational>, Vec<Rational>, CodecContract) {
    let baseline_audio = "AAC-LC stereo 48 kHz".to_owned();
    let mut codec = CodecContract {
        video_codec: "AVC/H.264".to_owned(),
        video_profile: "High".to_owned(),
        video_level: "5.1".to_owned(),
        chroma_format: "8-bit 4:2:0".to_owned(),
        video_bitrate_bps: 20_000_000,
        gop: "closed all-IDR; every frame random-access".to_owned(),
        audio_codec: baseline_audio,
        audio_bitrate_bps: 192_000,
        sample_entry: "avc1 + mp4a".to_owned(),
        timescale: 90_000,
        encoder_delay_samples: 0,
        encoder_padding_samples: 0,
        encoder_identity: "unselected-platform-encoder".to_owned(),
    };
    match fixture_id {
        FixtureId::QhdI => (2560, 1440, Some(Rational::new(30, 1)), Vec::new(), codec),
        FixtureId::QhdLgop => {
            "closed 60-frame GOP; IDR at each GOP boundary".clone_into(&mut codec.gop);
            codec.video_bitrate_bps = 28_000_000;
            (2560, 1440, Some(Rational::new(60, 1)), Vec::new(), codec)
        }
        FixtureId::ThreeKI => {
            "5.2".clone_into(&mut codec.video_level);
            codec.video_bitrate_bps = 30_000_000;
            (3072, 1728, Some(Rational::new(30, 1)), Vec::new(), codec)
        }
        FixtureId::ThreeKLgop => {
            "HEVC Main or AV1 Main capability variant".clone_into(&mut codec.video_codec);
            "Main".clone_into(&mut codec.video_profile);
            "capability-reported".clone_into(&mut codec.video_level);
            codec.video_bitrate_bps = 36_000_000;
            "closed 60-frame GOP; capability-reported random access".clone_into(&mut codec.gop);
            "AAC-LC or Opus stereo 48 kHz capability variant".clone_into(&mut codec.audio_codec);
            "capability-reported".clone_into(&mut codec.sample_entry);
            (3072, 1728, Some(Rational::new(60, 1)), Vec::new(), codec)
        }
        FixtureId::VfrAv => {
            "closed 30-frame GOP; exact per-frame PTS retained".clone_into(&mut codec.gop);
            (
                2560,
                1440,
                None,
                vec![Rational::new(1, 24), Rational::new(1, 30)],
                codec,
            )
        }
    }
}

fn deterministic_markers(duration: Rational) -> Vec<Marker> {
    let mut markers = Vec::new();
    let whole_seconds = duration.numerator / duration.denominator;
    let mut window_start = 0_u64;
    while window_start < whole_seconds {
        for offset in [1_u64, 7, 13, 19, 25] {
            let second = window_start + offset;
            if second < whole_seconds {
                markers.push(Marker {
                    name: format!("flash-chirp-{second:04}"),
                    pts: Rational::new(second, 1),
                    kind: "flash-and-chirp".to_owned(),
                });
            }
        }
        window_start += 30;
    }
    markers.push(Marker {
        name: "single-sample-impulse".to_owned(),
        pts: Rational::new(29_999, 1_000),
        kind: "single-sample-impulse".to_owned(),
    });
    markers.push(Marker {
        name: "end-guard".to_owned(),
        pts: difference(duration, Rational::new(1, u64::from(SAMPLE_RATE))),
        kind: "end-guard-flash-and-single-sample-impulse".to_owned(),
    });
    markers
}

/// One frame's source truth, intentionally represented only by metadata and a
/// digest so callers can stream without materializing a full frame buffer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FrameTruth {
    /// Zero-based source frame index.
    pub index: u64,
    /// Exact presentation time.
    pub pts: Rational,
    /// Exact presentation duration.
    pub duration: Rational,
    /// SHA-256 of deterministic frame semantics.
    pub semantic_sha256: String,
}

/// Returns one deterministic source RGBA pixel without allocating a decoded
/// frame. The moving gradient, color bars, and cut phase are intentionally
/// simple enough to reproduce independently in a CPU oracle.
///
/// # Panics
///
/// Panics when `(x, y)` is outside the fixture dimensions.
#[must_use]
pub fn rgba_pixel(recipe: &FixtureRecipe, index: u64, x: u32, y: u32) -> [u8; 4] {
    assert!(
        x < recipe.width && y < recipe.height,
        "pixel coordinates out of bounds"
    );
    rgba_pixel_with_marker(recipe, index, x, y, frame_contains_marker(recipe, index))
}

fn rgba_pixel_with_marker(
    recipe: &FixtureRecipe,
    index: u64,
    x: u32,
    y: u32,
    marker_active: bool,
) -> [u8; 4] {
    if marker_active && x < recipe.width / 8 && y < recipe.height / 8 {
        return [u8::MAX, u8::MAX, u8::MAX, u8::MAX];
    }
    let width_scale = u64::from(recipe.width.saturating_sub(1)).max(1);
    let height_scale = u64::from(recipe.height.saturating_sub(1)).max(1);
    let horizontal = u8::try_from(u64::from(x) * 255 / width_scale).expect("8-bit gradient");
    let vertical = u8::try_from(u64::from(y) * 255 / height_scale).expect("8-bit gradient");
    let motion = u8::try_from(index.wrapping_mul(17) % 256).expect("8-bit motion phase");
    let bar = u8::try_from(u64::from(x) * 8 / u64::from(recipe.width)).expect("color bar");
    let cut = u8::try_from((index / 90) % 2).expect("binary cut phase");
    let red = horizontal.wrapping_add(motion);
    let green = vertical.wrapping_add(bar * 29);
    let blue = if cut == 0 {
        horizontal ^ vertical ^ motion
    } else {
        horizontal.wrapping_add(vertical).wrapping_sub(motion)
    };
    [red, green, blue, u8::MAX]
}

fn frame_contains_marker(recipe: &FixtureRecipe, index: u64) -> bool {
    let pts = frame_pts(recipe, index);
    let end = pts
        .checked_add(frame_duration(recipe, index))
        .expect("fixture timestamp addition cannot overflow");
    recipe
        .markers
        .iter()
        .any(|marker| marker.pts >= pts && marker.pts < end)
}

/// Writes one raw, deterministic RGBA frame to `writer` and returns the full
/// decoded pixel SHA-256. The function never allocates a frame-sized buffer.
///
/// # Errors
///
/// Returns an I/O error from `writer`.
pub fn write_frame_rgba<W: Write>(
    writer: &mut W,
    recipe: &FixtureRecipe,
    index: u64,
) -> Result<String, std::io::Error> {
    let mut hasher = Sha256::new();
    let marker_active = frame_contains_marker(recipe, index);
    for y in 0..recipe.height {
        for x in 0..recipe.width {
            let pixel = rgba_pixel_with_marker(recipe, index, x, y, marker_active);
            writer.write_all(&pixel)?;
            hasher.update(pixel);
        }
    }
    Ok(hex_digest(hasher.finalize()))
}

/// One bounded audio block of source truth.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioBlockTruth {
    /// First 48 kHz sample index in the block.
    pub start_sample: u64,
    /// Stereo sample frames in the block.
    pub sample_frames: u32,
    /// SHA-256 of interleaved little-endian i32 samples.
    pub semantic_sha256: String,
}

/// Streams deterministic fixture truth in bounded frame and audio units.
#[derive(Clone, Debug)]
pub struct FixtureStream {
    recipe: FixtureRecipe,
}

impl FixtureStream {
    fn new(recipe: FixtureRecipe) -> Self {
        Self { recipe }
    }

    /// Streams frame truth one frame at a time.
    #[must_use]
    pub fn frames(&self) -> FrameTruths {
        FrameTruths {
            recipe: self.recipe.clone(),
            next_index: 0,
            next_pts: Rational::new(0, 1),
        }
    }

    /// Streams audio truth in bounded blocks.
    ///
    /// # Panics
    ///
    /// Panics when `sample_frames` is zero.
    #[must_use]
    pub fn audio_blocks(&self, sample_frames: u32) -> AudioTruths {
        assert!(sample_frames > 0, "audio block size must not be zero");
        AudioTruths {
            recipe: self.recipe.clone(),
            total_samples: self.recipe.duration.numerator * u64::from(SAMPLE_RATE)
                / self.recipe.duration.denominator,
            next_sample: 0,
            sample_frames,
        }
    }

    /// Returns a SHA-256 digest of metadata plus all frame/audio truth.
    #[must_use]
    pub fn semantic_digest(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.recipe.fixture_id.as_bytes());
        hasher.update(self.recipe.duration.canonical().as_bytes());
        hasher.update(self.recipe.color_metadata.as_bytes());
        for frame in self.frames() {
            hasher.update(frame.index.to_le_bytes());
            hasher.update(frame.pts.canonical().as_bytes());
            hasher.update(frame.duration.canonical().as_bytes());
            hasher.update(frame.semantic_sha256.as_bytes());
        }
        for block in self.audio_blocks(1_024) {
            hasher.update(block.start_sample.to_le_bytes());
            hasher.update(block.sample_frames.to_le_bytes());
            hasher.update(block.semantic_sha256.as_bytes());
        }
        hex_digest(hasher.finalize())
    }
}

/// Frame truth iterator.
pub struct FrameTruths {
    recipe: FixtureRecipe,
    next_index: u64,
    next_pts: Rational,
}

impl Iterator for FrameTruths {
    type Item = FrameTruth;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next_pts >= self.recipe.duration {
            return None;
        }
        let nominal_duration = frame_duration(&self.recipe, self.next_index);
        let remaining = difference(self.recipe.duration, self.next_pts);
        let duration = nominal_duration.min(remaining);
        let frame = FrameTruth {
            index: self.next_index,
            pts: self.next_pts,
            duration,
            semantic_sha256: frame_digest(&self.recipe, self.next_index, self.next_pts, duration),
        };
        self.next_index += 1;
        self.next_pts = self.next_pts.checked_add(duration)?;
        Some(frame)
    }
}

fn frame_duration(recipe: &FixtureRecipe, index: u64) -> Rational {
    if let Some(frame_rate) = recipe.frame_rate {
        return Rational::new(frame_rate.denominator, frame_rate.numerator);
    }
    let index = usize::try_from(index).expect("fixture frame index fits usize");
    recipe.vfr_frame_durations[index % recipe.vfr_frame_durations.len()]
}

fn frame_pts(recipe: &FixtureRecipe, index: u64) -> Rational {
    if let Some(frame_rate) = recipe.frame_rate {
        return Rational::new(
            index
                .checked_mul(frame_rate.denominator)
                .expect("fixture frame PTS cannot overflow"),
            frame_rate.numerator,
        );
    }
    let pair_index = index / 2;
    let pair_duration = recipe.vfr_frame_durations[0]
        .checked_add(recipe.vfr_frame_durations[1])
        .expect("fixture VFR cadence cannot overflow");
    let pair_pts = Rational::new(
        pair_index
            .checked_mul(pair_duration.numerator)
            .expect("fixture VFR PTS cannot overflow"),
        pair_duration.denominator,
    );
    if index.is_multiple_of(2) {
        pair_pts
    } else {
        pair_pts
            .checked_add(recipe.vfr_frame_durations[0])
            .expect("fixture VFR PTS cannot overflow")
    }
}

fn frame_digest(recipe: &FixtureRecipe, index: u64, pts: Rational, duration: Rational) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"stitch-frame-truth-v1");
    hasher.update(recipe.fixture_id.as_bytes());
    hasher.update(recipe.width.to_le_bytes());
    hasher.update(recipe.height.to_le_bytes());
    hasher.update(index.to_le_bytes());
    hasher.update(pts.canonical().as_bytes());
    hasher.update(duration.canonical().as_bytes());
    hasher.update(recipe.color_metadata.as_bytes());
    for marker in &recipe.markers {
        if marker.pts == pts {
            hasher.update(marker.name.as_bytes());
            hasher.update(marker.kind.as_bytes());
        }
    }
    hex_digest(hasher.finalize())
}

/// Audio truth iterator.
pub struct AudioTruths {
    recipe: FixtureRecipe,
    total_samples: u64,
    next_sample: u64,
    sample_frames: u32,
}

impl Iterator for AudioTruths {
    type Item = AudioBlockTruth;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next_sample >= self.total_samples {
            return None;
        }
        let remaining = self.total_samples - self.next_sample;
        let sample_frames = u32::try_from(remaining.min(u64::from(self.sample_frames)))
            .expect("bounded audio block fits u32");
        let start_sample = self.next_sample;
        self.next_sample += u64::from(sample_frames);
        Some(AudioBlockTruth {
            start_sample,
            sample_frames,
            semantic_sha256: audio_digest(&self.recipe, start_sample, sample_frames),
        })
    }
}

fn audio_digest(recipe: &FixtureRecipe, start_sample: u64, sample_frames: u32) -> String {
    let mut hasher = Sha256::new();
    for sample_index in start_sample..start_sample + u64::from(sample_frames) {
        for channel in 0..2 {
            hasher.update(audio_sample(recipe, sample_index, channel).to_le_bytes());
        }
    }
    hex_digest(hasher.finalize())
}

/// Returns one deterministic 32-bit PCM sample without allocating an audio
/// buffer. The recipe's marker schedule is part of the generated signal.
#[must_use]
pub fn audio_sample(recipe: &FixtureRecipe, sample_index: u64, channel: u8) -> i32 {
    base_audio_sample(sample_index, channel)
        .saturating_add(audio_marker_signal(recipe, sample_index))
}

fn base_audio_sample(sample_index: u64, channel: u8) -> i32 {
    let state = sample_index
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(u64::from(channel) * 1_442_695_040_888_963_407);
    let shaped = ((state >> 32) as i32).wrapping_sub(i32::MAX / 2);
    shaped.saturating_div(2)
}

fn audio_marker_signal(recipe: &FixtureRecipe, sample_index: u64) -> i32 {
    let total_samples =
        recipe.duration.numerator * u64::from(SAMPLE_RATE) / recipe.duration.denominator;
    if sample_index == total_samples.saturating_sub(1) || sample_index == 29_999_u64 * 48 {
        return i32::MAX / 4;
    }
    let second = sample_index / u64::from(SAMPLE_RATE);
    let sample_in_second = sample_index % u64::from(SAMPLE_RATE);
    if second < total_samples / u64::from(SAMPLE_RATE)
        && matches!(second % 30, 1 | 7 | 13 | 19 | 25)
        && sample_in_second < 480
    {
        let phase = i32::try_from(sample_in_second).expect("chirp phase fits i32");
        return (i32::MAX / 16) + phase * (i32::MAX / (480 * 32));
    }
    0
}

/// Writes a companion, lossless 48 kHz stereo 32-bit PCM WAV oracle in a
/// streaming fashion and returns its SHA-256 digest.
///
/// # Errors
///
/// Returns an I/O error from the supplied writer.
///
/// # Panics
///
/// Panics only if the fixed 600-second WAV no longer fits the RIFF 32-bit
/// length fields, which would be a source-contract bug.
pub fn write_wav_oracle<W: Write + Seek>(
    writer: &mut W,
    recipe: &FixtureRecipe,
) -> Result<String, std::io::Error> {
    let total_sample_frames =
        recipe.duration.numerator * u64::from(SAMPLE_RATE) / recipe.duration.denominator;
    let bytes_per_sample_u64 =
        u64::try_from(std::mem::size_of::<i32>()).expect("i32 size fits u64");
    let data_size = total_sample_frames * 2 * bytes_per_sample_u64;
    let riff_size = 36_u64 + data_size;
    writer.write_all(b"RIFF")?;
    writer.write_all(
        &u32::try_from(riff_size)
            .expect("release WAV fits RIFF u32")
            .to_le_bytes(),
    )?;
    writer.write_all(b"WAVEfmt ")?;
    writer.write_all(&16_u32.to_le_bytes())?;
    writer.write_all(&1_u16.to_le_bytes())?;
    writer.write_all(&2_u16.to_le_bytes())?;
    writer.write_all(&SAMPLE_RATE.to_le_bytes())?;
    let bytes_per_sample = u32::try_from(std::mem::size_of::<i32>()).expect("i32 size fits u32");
    let byte_rate = SAMPLE_RATE * 2 * bytes_per_sample;
    writer.write_all(&byte_rate.to_le_bytes())?;
    writer.write_all(&8_u16.to_le_bytes())?;
    writer.write_all(&32_u16.to_le_bytes())?;
    writer.write_all(b"data")?;
    writer.write_all(
        &u32::try_from(data_size)
            .expect("release WAV fits data u32")
            .to_le_bytes(),
    )?;

    let mut hasher = Sha256::new();
    for sample_index in 0..total_sample_frames {
        for channel in 0..2 {
            let sample = audio_sample(recipe, sample_index, channel).to_le_bytes();
            writer.write_all(&sample)?;
            hasher.update(sample);
        }
    }
    writer.seek(SeekFrom::End(0))?;
    Ok(hex_digest(hasher.finalize()))
}

fn hex_digest(bytes: impl AsRef<[u8]>) -> String {
    let mut output = String::with_capacity(64);
    for byte in bytes.as_ref() {
        write!(&mut output, "{byte:02x}").expect("writing into String cannot fail");
    }
    output
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};

    use serde_json::Value;

    use super::{
        AudioBitrateTier, CapabilityResult, CodecProperties, CodecRequest, CorpusDuration,
        EncodedArtifactManifest, FixtureId, FixtureRecipe, Rational, SAMPLE_RATE, SampleLedger,
        audio_sample, base_audio_sample, frame_contains_marker, write_frame_rgba, write_wav_oracle,
    };

    #[test]
    fn baseline_recipes_have_platform_safe_audio_contract() {
        for fixture_id in [
            FixtureId::QhdI,
            FixtureId::QhdLgop,
            FixtureId::ThreeKI,
            FixtureId::VfrAv,
        ] {
            let recipe = FixtureRecipe::new(fixture_id, CorpusDuration::Development);
            assert_eq!(recipe.codec.audio_bitrate_bps, 192_000);
            assert_eq!(recipe.codec.timescale, 90_000);
            assert!(recipe.color_metadata.starts_with("Rec.709"));
        }
    }

    #[test]
    fn frame_and_audio_streams_are_repeatable_without_materialization() {
        let recipe = FixtureRecipe::new(FixtureId::VfrAv, CorpusDuration::Development);
        let first = recipe.stream().semantic_digest();
        let second = recipe.stream().semantic_digest();
        assert_eq!(first, second);
        assert_eq!(recipe.stream().frames().count(), 800);
        assert_eq!(recipe.stream().audio_blocks(1_024).count(), 1_407);
        let final_end = recipe
            .stream()
            .frames()
            .last()
            .expect("fixture has frames")
            .pts
            .checked_add(
                recipe
                    .stream()
                    .frames()
                    .last()
                    .expect("fixture has frames")
                    .duration,
            )
            .expect("end timestamp is representable");
        assert_eq!(final_end, Rational::new(30, 1));
    }

    #[test]
    fn wav_writer_emits_exact_pcm_data_length() {
        let mut output = Cursor::new(Vec::new());
        let recipe = FixtureRecipe::new(FixtureId::QhdI, CorpusDuration::Development);
        let digest = write_wav_oracle(&mut output, &recipe).expect("WAV write succeeds");
        let expected_data_bytes = u64::from(SAMPLE_RATE) * 30 * 2 * 4;
        assert_eq!(
            u64::try_from(output.get_ref().len()).expect("length fits u64"),
            44 + expected_data_bytes
        );
        assert_eq!(digest.len(), 64);
    }

    #[test]
    fn rational_timestamp_is_normalized() {
        assert_eq!(Rational::new(90_000, 30).canonical(), "3000/1");
    }

    #[test]
    fn checked_in_corpus_pins_every_generated_semantic_digest() {
        let corpus: Value = serde_json::from_str(include_str!("../recipes/corpus-v1.json"))
            .expect("corpus recipe is valid JSON");
        for fixture in corpus["fixtures"].as_array().expect("fixtures array") {
            let fixture_id = match fixture["fixture_id"].as_str().expect("fixture id") {
                "QHD-I" => FixtureId::QhdI,
                "QHD-LGOP" => FixtureId::QhdLgop,
                "3K-I" => FixtureId::ThreeKI,
                "3K-LGOP" => FixtureId::ThreeKLgop,
                "VFR-AV" => FixtureId::VfrAv,
                other => panic!("unknown fixture id {other}"),
            };
            for corpus_duration in fixture["corpora"].as_array().expect("corpora array") {
                let duration = match corpus_duration["name"].as_str().expect("corpus name") {
                    "development" => CorpusDuration::Development,
                    "release" => CorpusDuration::Release,
                    other => panic!("unknown corpus duration {other}"),
                };
                let generated = FixtureRecipe::new(fixture_id, duration);
                assert_eq!(
                    corpus_duration["semantic_sha256"].as_str(),
                    Some(generated.semantic_sha256.as_str())
                );
                assert!(corpus_duration["artifact_sha256"].is_null());
            }
        }
    }

    #[test]
    fn raw_frame_writer_streams_without_a_frame_buffer() {
        struct CountingWriter(usize);

        impl Write for CountingWriter {
            fn write(&mut self, buffer: &[u8]) -> Result<usize, std::io::Error> {
                self.0 += buffer.len();
                Ok(buffer.len())
            }

            fn flush(&mut self) -> Result<(), std::io::Error> {
                Ok(())
            }
        }

        let recipe = FixtureRecipe::new(FixtureId::QhdI, CorpusDuration::Development);
        let mut writer = CountingWriter(0);
        let digest = write_frame_rgba(&mut writer, &recipe, 0).expect("raw frame streams");
        assert_eq!(
            writer.0,
            usize::try_from(recipe.width * recipe.height * 4).expect("size fits")
        );
        assert_eq!(digest.len(), 64);
    }

    #[test]
    fn marker_events_change_pixels_and_samples_through_the_release_end_guard() {
        let recipe = FixtureRecipe::new(FixtureId::VfrAv, CorpusDuration::Release);
        assert_eq!(recipe.stream().frames().count(), 16_000);
        assert!(
            recipe.markers.len() > 90,
            "release markers span the full duration"
        );
        for marker in &recipe.markers {
            let marker_frame = recipe
                .stream()
                .frames()
                .find(|frame| {
                    marker.pts >= frame.pts
                        && marker.pts < frame.pts.checked_add(frame.duration).expect("valid end")
                })
                .expect("each marker is inside a video frame");
            assert!(frame_contains_marker(&recipe, marker_frame.index));
            assert_eq!(
                super::rgba_pixel(&recipe, marker_frame.index, 0, 0),
                [255; 4]
            );
            let marker_sample =
                marker.pts.numerator * u64::from(SAMPLE_RATE) / marker.pts.denominator;
            assert_ne!(
                audio_sample(&recipe, marker_sample, 0),
                base_audio_sample(marker_sample, 0),
                "{} must change the generated audio signal",
                marker.name
            );
        }
    }

    fn codec_properties(audio_bitrate_bps: u32) -> CodecProperties {
        CodecProperties {
            video_codec: "AVC/H.264".to_owned(),
            video_profile: "High".to_owned(),
            video_level: "5.1".to_owned(),
            video_bitrate_bps: 20_000_000,
            gop: "closed all-IDR".to_owned(),
            audio_codec: "AAC-LC".to_owned(),
            audio_bitrate_bps,
            sample_entry: "avc1 + mp4a".to_owned(),
            configuration_sha256: "a".repeat(64),
            video_timescale: 90_000,
            audio_timescale: SAMPLE_RATE,
            encoder_delay_samples: 0,
            encoder_padding_samples: 0,
        }
    }

    #[test]
    fn capability_and_artifact_schemas_enforce_the_192_and_320_contract() {
        let mandatory = CodecRequest {
            properties: codec_properties(192_000),
            audio_bitrate_tier: AudioBitrateTier::MandatoryBaseline,
        };
        assert!(mandatory.validate().is_ok());
        let optional = CodecRequest {
            properties: codec_properties(320_000),
            audio_bitrate_tier: AudioBitrateTier::OptionalCapability,
        };
        assert!(optional.validate().is_ok());
        let invalid = CodecRequest {
            properties: codec_properties(320_000),
            audio_bitrate_tier: AudioBitrateTier::MandatoryBaseline,
        };
        assert!(invalid.validate().is_err());
        let capability = CapabilityResult {
            schema_version: 1,
            fixture_id: "QHD-I".to_owned(),
            requested: mandatory.clone(),
            supported: true,
            actual: Some(codec_properties(192_000)),
            platform_encoder_identity: "test-encoder".to_owned(),
            diagnostic: None,
        };
        assert!(capability.validate().is_ok());
        let artifact = EncodedArtifactManifest {
            schema_version: 1,
            fixture_id: "QHD-I".to_owned(),
            source_recipe_sha256: "b".repeat(64),
            requested: mandatory,
            actual: codec_properties(192_000),
            sample_ledger: SampleLedger {
                video_frame_count: 900,
                audio_sample_frames: 1_440_000,
                video_timing_sha256: "c".repeat(64),
                audio_timing_sha256: "d".repeat(64),
            },
            artifact_sha256: "e".repeat(64),
            platform_encoder_identity: "test-encoder".to_owned(),
            license: "controlled-output".to_owned(),
        };
        assert!(artifact.validate().is_ok());
    }
}
