//! A deterministic, implementation-independent benchmark evidence harness.
//!
//! The crate deliberately models evidence rather than editor behavior.  An editor
//! adapter supplies measured repetitions; this crate schedules them, validates the
//! disclosed environment, preserves failures, and emits portable JSON evidence.

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const REQUIRED_TRACE_STAGES: [TraceStage; 16] = [
    TraceStage::Probe,
    TraceStage::Io,
    TraceStage::Demux,
    TraceStage::Index,
    TraceStage::Decode,
    TraceStage::Upload,
    TraceStage::Node,
    TraceStage::Composite,
    TraceStage::Audio,
    TraceStage::Present,
    TraceStage::Encode,
    TraceStage::Mux,
    TraceStage::Cache,
    TraceStage::Cancel,
    TraceStage::Error,
    TraceStage::Finalize,
];

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Variant {
    Candidate,
    Baseline,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheTemperature {
    Cold,
    Warm,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScheduleItem {
    pub sequence: u32,
    pub pair: u32,
    pub variant: Variant,
    pub temperature: CacheTemperature,
}

/// Builds a stable randomized schedule. Every pair has exactly one candidate and
/// one baseline run at each cache temperature, and the final order is shuffled
/// with a tiny explicit PRNG so it remains stable across Rust releases.
///
/// # Panics
///
/// Panics only when the requested schedule cannot fit in the current platform's
/// address space.
#[must_use]
pub fn seeded_interleaving(seed: u64, pairs: u32) -> Vec<ScheduleItem> {
    let mut items = Vec::with_capacity((pairs as usize) * 4);
    for pair in 0..pairs {
        for temperature in [CacheTemperature::Cold, CacheTemperature::Warm] {
            for variant in [Variant::Candidate, Variant::Baseline] {
                items.push(ScheduleItem {
                    sequence: 0,
                    pair,
                    variant,
                    temperature,
                });
            }
        }
    }

    let mut random = SplitMix64::new(seed);
    for index in (1..items.len()).rev() {
        let bound = u64::try_from(index + 1).expect("schedule length fits u64");
        let swap_index = usize::try_from(random.next() % bound).expect("bounded index fits usize");
        items.swap(index, swap_index);
    }
    for (sequence, item) in items.iter_mut().enumerate() {
        item.sequence = u32::try_from(sequence).expect("schedule fits u32");
    }
    items
}

#[derive(Clone, Debug)]
struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut value = self.state;
        value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        value ^ (value >> 31)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DisclosureBundle {
    pub source: SourceDisclosure,
    pub artifacts: ArtifactDigests,
    pub machine: MachineDisclosure,
    pub codec: CodecDisclosure,
    pub audio: AudioDisclosure,
    pub runtime: RuntimeDisclosure,
    pub references: EvidenceReferences,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SourceDisclosure {
    pub commit: String,
    pub dirty: bool,
    pub build_flags: Vec<String>,
    pub lockfile_sha256: String,
    pub sbom_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ArtifactDigests {
    pub fixture_sha256: String,
    pub project_sha256: String,
    pub proxy_sha256: String,
    pub render_sha256: String,
    pub oracle_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MachineDisclosure {
    pub os: OsDisclosure,
    pub cpu: CpuDisclosure,
    pub memory: MemoryDisclosure,
    pub gpu: GpuDisclosure,
    pub storage: StorageDisclosure,
    pub display: DisplayDisclosure,
    pub power: String,
    pub thermal: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OsDisclosure {
    pub name: String,
    pub build: String,
    pub security_updates: String,
    pub graphics_updates: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CpuDisclosure {
    pub model: String,
    pub physical_cores: u32,
    pub logical_cores: u32,
    pub scheduler: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MemoryDisclosure {
    pub bytes: u64,
    pub speed_mhz: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GpuDisclosure {
    pub model: String,
    pub driver: String,
    pub vram_or_unified_budget_bytes: u64,
    pub metal_feature_set: String,
    pub d3d_feature_level: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct StorageDisclosure {
    pub model: String,
    pub interface: String,
    pub filesystem: String,
    pub free_bytes: u64,
    pub project_volume: String,
    pub cache_volume: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DisplayDisclosure {
    pub width_px: u32,
    pub height_px: u32,
    pub refresh_millihz: u32,
    pub dpi_milli: u32,
    pub qt_backend: String,
}

impl MachineDisclosure {
    #[must_use]
    pub fn fingerprint(&self) -> String {
        stable_digest(self)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CodecDisclosure {
    pub video_codec: String,
    pub profile: String,
    pub level: String,
    pub sample_entry: String,
    pub configuration: String,
    pub decode_path: CodecPath,
    pub encode_path: CodecPath,
    pub surface_format: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CodecPath {
    Hardware,
    Software,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AudioDisclosure {
    pub device: String,
    pub driver: String,
    pub sample_rate_hz: u32,
    pub buffer_frames: u32,
    pub offset_frames: i64,
    pub mode: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RuntimeDisclosure {
    pub cache_state: String,
    pub background_load: String,
    pub network_state: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EvidenceReferences {
    pub trace: String,
    pub render_manifest: String,
    pub failure_bundle: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RawRepetition {
    pub schedule: ScheduleItem,
    pub elapsed_ns: Option<u64>,
    pub correctness_passed: bool,
    pub adapter_error: Option<String>,
    pub disclosure: DisclosureBundle,
    pub resource_observation: ResourceObservation,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ResourceObservation {
    pub process_rss_bytes: u64,
    pub gpu_memory_bytes: Option<u64>,
    pub queue_items: u64,
    pub queue_bytes: u64,
    pub resource_tokens: BTreeMap<String, u64>,
    pub codec_path: CodecPath,
}

impl RawRepetition {
    #[must_use]
    pub fn is_valid_measurement(&self) -> bool {
        self.correctness_passed && self.adapter_error.is_none() && self.elapsed_ns.is_some()
    }

    fn identity(&self) -> (u32, Variant, CacheTemperature) {
        (
            self.schedule.pair,
            self.schedule.variant,
            self.schedule.temperature,
        )
    }

    fn pairing_identity(&self) -> String {
        stable_digest(&PairingIdentity {
            fixture_sha256: &self.disclosure.artifacts.fixture_sha256,
            project_sha256: &self.disclosure.artifacts.project_sha256,
            proxy_sha256: &self.disclosure.artifacts.proxy_sha256,
            oracle_sha256: &self.disclosure.artifacts.oracle_sha256,
            machine: &self.disclosure.machine,
            codec: &self.disclosure.codec,
            audio: &self.disclosure.audio,
            runtime: &self.disclosure.runtime,
        })
    }
}

#[derive(Serialize)]
struct PairingIdentity<'a> {
    fixture_sha256: &'a str,
    project_sha256: &'a str,
    proxy_sha256: &'a str,
    oracle_sha256: &'a str,
    machine: &'a MachineDisclosure,
    codec: &'a CodecDisclosure,
    audio: &'a AudioDisclosure,
    runtime: &'a RuntimeDisclosure,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FailureRecord {
    pub sequence: u32,
    pub pair: u32,
    pub variant: Variant,
    pub correctness_passed: bool,
    pub adapter_error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BenchmarkBundle {
    pub schema_version: u32,
    pub seed: u64,
    pub schedule: Vec<ScheduleItem>,
    pub raw_repetitions: Vec<RawRepetition>,
    pub failures: Vec<FailureRecord>,
    pub trace: TraceDocument,
    pub render_manifest: RenderManifest,
}

impl BenchmarkBundle {
    #[must_use]
    pub fn new(
        seed: u64,
        schedule: Vec<ScheduleItem>,
        raw_repetitions: Vec<RawRepetition>,
        trace: TraceDocument,
        render_manifest: RenderManifest,
    ) -> Self {
        let failures = raw_repetitions
            .iter()
            .filter(|run| !run.correctness_passed || run.adapter_error.is_some())
            .map(|run| FailureRecord {
                sequence: run.schedule.sequence,
                pair: run.schedule.pair,
                variant: run.schedule.variant,
                correctness_passed: run.correctness_passed,
                adapter_error: run.adapter_error.clone(),
            })
            .collect();
        Self {
            schema_version: 1,
            seed,
            schedule,
            raw_repetitions,
            failures,
            trace,
            render_manifest,
        }
    }

    /// Validates the complete schedule, raw evidence, failure retention, trace,
    /// and render manifest.
    ///
    /// # Errors
    ///
    /// Returns an error when any required evidence is incomplete or inconsistent.
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.schema_version != 1 {
            return Err(ValidationError::SchemaVersion(self.schema_version));
        }
        validate_schedule(&self.schedule)?;
        if self.raw_repetitions.len() != self.schedule.len() {
            return Err(ValidationError::DroppedRepetitions {
                scheduled: self.schedule.len(),
                recorded: self.raw_repetitions.len(),
            });
        }
        let scheduled: BTreeMap<_, _> = self
            .schedule
            .iter()
            .map(|item| ((item.pair, item.variant, item.temperature), item.sequence))
            .collect();
        let recorded: BTreeSet<_> = self
            .raw_repetitions
            .iter()
            .map(RawRepetition::identity)
            .collect();
        if scheduled.keys().copied().collect::<BTreeSet<_>>() != recorded {
            return Err(ValidationError::ScheduleMismatch);
        }
        for run in &self.raw_repetitions {
            if scheduled.get(&run.identity()) != Some(&run.schedule.sequence) {
                return Err(ValidationError::ScheduleMismatch);
            }
            validate_disclosure(&run.disclosure)?;
            if run.correctness_passed && run.adapter_error.is_none() && run.elapsed_ns.is_none() {
                return Err(ValidationError::MissingElapsedTime(run.schedule.sequence));
            }
        }
        let expected_failures: Vec<_> = self
            .raw_repetitions
            .iter()
            .filter(|run| !run.correctness_passed || run.adapter_error.is_some())
            .map(|run| FailureRecord {
                sequence: run.schedule.sequence,
                pair: run.schedule.pair,
                variant: run.schedule.variant,
                correctness_passed: run.correctness_passed,
                adapter_error: run.adapter_error.clone(),
            })
            .collect();
        if self.failures != expected_failures {
            return Err(ValidationError::FailureRetentionMismatch);
        }
        self.trace.validate()?;
        self.render_manifest.validate()?;
        Ok(())
    }

    /// Serializes the evidence using stable struct and map ordering.
    ///
    /// # Errors
    ///
    /// Returns an error when JSON serialization fails.
    pub fn deterministic_json(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec_pretty(self)
    }
}

fn validate_schedule(schedule: &[ScheduleItem]) -> Result<(), ValidationError> {
    let mut identities = BTreeSet::new();
    let mut sequences = BTreeSet::new();
    let mut expected_per_pair = BTreeMap::<u32, BTreeSet<(Variant, CacheTemperature)>>::new();
    for item in schedule {
        if !identities.insert((item.pair, item.variant, item.temperature)) {
            return Err(ValidationError::DuplicateScheduleItem);
        }
        if !sequences.insert(item.sequence) {
            return Err(ValidationError::DuplicateSequence);
        }
        expected_per_pair
            .entry(item.pair)
            .or_default()
            .insert((item.variant, item.temperature));
    }
    for entries in expected_per_pair.values() {
        if entries.len() != 4 {
            return Err(ValidationError::ScheduleMismatch);
        }
    }
    if sequences
        .iter()
        .copied()
        .ne(0..u32::try_from(schedule.len()).expect("schedule fits u32"))
    {
        return Err(ValidationError::ScheduleMismatch);
    }
    Ok(())
}

fn validate_disclosure(disclosure: &DisclosureBundle) -> Result<(), ValidationError> {
    validate_source_and_artifacts(disclosure)?;
    validate_machine_strings(&disclosure.machine)?;
    validate_machine_numbers(&disclosure.machine)?;
    validate_codec_audio_runtime(disclosure)
}

fn validate_source_and_artifacts(disclosure: &DisclosureBundle) -> Result<(), ValidationError> {
    require_strings(&[
        ("source.commit", &disclosure.source.commit),
        ("source.lockfile_sha256", &disclosure.source.lockfile_sha256),
        ("source.sbom_sha256", &disclosure.source.sbom_sha256),
        (
            "artifacts.fixture_sha256",
            &disclosure.artifacts.fixture_sha256,
        ),
        (
            "artifacts.project_sha256",
            &disclosure.artifacts.project_sha256,
        ),
        ("artifacts.proxy_sha256", &disclosure.artifacts.proxy_sha256),
        (
            "artifacts.render_sha256",
            &disclosure.artifacts.render_sha256,
        ),
        (
            "artifacts.oracle_sha256",
            &disclosure.artifacts.oracle_sha256,
        ),
    ])?;
    if disclosure.source.build_flags.is_empty()
        || disclosure
            .source
            .build_flags
            .iter()
            .any(|flag| flag.trim().is_empty())
    {
        return Err(ValidationError::MissingDisclosure("source.build_flags"));
    }
    Ok(())
}

fn validate_machine_strings(machine: &MachineDisclosure) -> Result<(), ValidationError> {
    require_strings(&[
        ("machine.os.name", &machine.os.name),
        ("machine.os.build", &machine.os.build),
        ("machine.os.security_updates", &machine.os.security_updates),
        ("machine.os.graphics_updates", &machine.os.graphics_updates),
        ("machine.cpu.model", &machine.cpu.model),
        ("machine.cpu.scheduler", &machine.cpu.scheduler),
        ("machine.gpu.model", &machine.gpu.model),
        ("machine.gpu.driver", &machine.gpu.driver),
        (
            "machine.gpu.metal_feature_set",
            &machine.gpu.metal_feature_set,
        ),
        (
            "machine.gpu.d3d_feature_level",
            &machine.gpu.d3d_feature_level,
        ),
        ("machine.storage.model", &machine.storage.model),
        ("machine.storage.interface", &machine.storage.interface),
        ("machine.storage.filesystem", &machine.storage.filesystem),
        (
            "machine.storage.project_volume",
            &machine.storage.project_volume,
        ),
        (
            "machine.storage.cache_volume",
            &machine.storage.cache_volume,
        ),
        ("machine.display.qt_backend", &machine.display.qt_backend),
        ("machine.power", &machine.power),
        ("machine.thermal", &machine.thermal),
    ])
}

fn validate_machine_numbers(machine: &MachineDisclosure) -> Result<(), ValidationError> {
    require_nonzero_u32(&[
        ("machine.cpu.physical_cores", machine.cpu.physical_cores),
        ("machine.cpu.logical_cores", machine.cpu.logical_cores),
        ("machine.display.width_px", machine.display.width_px),
        ("machine.display.height_px", machine.display.height_px),
        (
            "machine.display.refresh_millihz",
            machine.display.refresh_millihz,
        ),
        ("machine.display.dpi_milli", machine.display.dpi_milli),
    ])?;
    require_nonzero_u64(&[
        ("machine.memory.bytes", machine.memory.bytes),
        ("machine.memory.speed_mhz", machine.memory.speed_mhz),
        (
            "machine.gpu.vram_or_unified_budget_bytes",
            machine.gpu.vram_or_unified_budget_bytes,
        ),
        ("machine.storage.free_bytes", machine.storage.free_bytes),
    ])
}

fn validate_codec_audio_runtime(disclosure: &DisclosureBundle) -> Result<(), ValidationError> {
    require_strings(&[
        ("codec.video_codec", &disclosure.codec.video_codec),
        ("codec.profile", &disclosure.codec.profile),
        ("codec.level", &disclosure.codec.level),
        ("codec.sample_entry", &disclosure.codec.sample_entry),
        ("codec.configuration", &disclosure.codec.configuration),
        ("codec.surface_format", &disclosure.codec.surface_format),
        ("audio.device", &disclosure.audio.device),
        ("audio.driver", &disclosure.audio.driver),
        ("audio.mode", &disclosure.audio.mode),
        ("runtime.cache_state", &disclosure.runtime.cache_state),
        (
            "runtime.background_load",
            &disclosure.runtime.background_load,
        ),
        ("runtime.network_state", &disclosure.runtime.network_state),
        ("references.trace", &disclosure.references.trace),
        (
            "references.render_manifest",
            &disclosure.references.render_manifest,
        ),
        (
            "references.failure_bundle",
            &disclosure.references.failure_bundle,
        ),
    ])?;
    require_nonzero_u32(&[
        ("audio.sample_rate_hz", disclosure.audio.sample_rate_hz),
        ("audio.buffer_frames", disclosure.audio.buffer_frames),
    ])
}

fn require_strings(required: &[(&'static str, &str)]) -> Result<(), ValidationError> {
    for (field, value) in required {
        if value.trim().is_empty() {
            return Err(ValidationError::MissingDisclosure(field));
        }
    }
    Ok(())
}

fn require_nonzero_u32(required: &[(&'static str, u32)]) -> Result<(), ValidationError> {
    for (field, value) in required {
        if *value == 0 {
            return Err(ValidationError::MissingDisclosure(field));
        }
    }
    Ok(())
}

fn require_nonzero_u64(required: &[(&'static str, u64)]) -> Result<(), ValidationError> {
    for (field, value) in required {
        if *value == 0 {
            return Err(ValidationError::MissingDisclosure(field));
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ValidationError {
    SchemaVersion(u32),
    DuplicateScheduleItem,
    DuplicateSequence,
    DroppedRepetitions { scheduled: usize, recorded: usize },
    ScheduleMismatch,
    FailureRetentionMismatch,
    MissingElapsedTime(u32),
    MissingDisclosure(&'static str),
    InvalidTrace(&'static str),
    InvalidRenderManifest(&'static str),
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SchemaVersion(version) => {
                write!(formatter, "unsupported schema version {version}")
            }
            Self::DuplicateScheduleItem => write!(formatter, "duplicate schedule item"),
            Self::DuplicateSequence => write!(formatter, "duplicate schedule sequence"),
            Self::DroppedRepetitions {
                scheduled,
                recorded,
            } => {
                write!(
                    formatter,
                    "dropped repetitions: scheduled {scheduled}, recorded {recorded}"
                )
            }
            Self::ScheduleMismatch => {
                write!(formatter, "recorded repetitions do not match schedule")
            }
            Self::FailureRetentionMismatch => {
                write!(formatter, "failure list does not retain every failure")
            }
            Self::MissingElapsedTime(sequence) => {
                write!(formatter, "valid repetition {sequence} has no duration")
            }
            Self::MissingDisclosure(field) => write!(formatter, "missing disclosure: {field}"),
            Self::InvalidTrace(reason) => write!(formatter, "invalid trace: {reason}"),
            Self::InvalidRenderManifest(reason) => {
                write!(formatter, "invalid render manifest: {reason}")
            }
        }
    }
}

impl std::error::Error for ValidationError {}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceStage {
    Probe,
    Io,
    Demux,
    Index,
    Decode,
    Upload,
    Node,
    Composite,
    Audio,
    Present,
    Encode,
    Mux,
    Cache,
    Cancel,
    Error,
    Finalize,
}

/// A non-negative, exact rational quantity such as a canonical timeline time.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ExactRational {
    pub numerator: u64,
    pub denominator: u64,
}

/// A signed, exact rational quantity such as a rounding error.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ExactSignedRational {
    pub numerator: i64,
    pub denominator: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RoundingMode {
    Floor,
    Ceil,
    NearestTiesToEven,
    TowardZero,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AdapterTiming {
    pub tick: u64,
    pub timescale: u64,
    pub rounding: RoundingMode,
    pub rounding_error: ExactSignedRational,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TraceContext {
    pub trace_revision: String,
    pub project_revision: String,
    pub semantic_digest: String,
    pub execution_digest: String,
    pub mix_digest: String,
    pub quality: String,
    pub generation: u64,
    pub canonical_time: ExactRational,
    pub adapter_timing: AdapterTiming,
    pub adapter_error: Option<String>,
    pub queue_items: u64,
    pub queue_bytes: u64,
    pub resource_tokens: BTreeMap<String, u64>,
    pub process_rss_bytes: u64,
    pub gpu_memory_bytes: Option<u64>,
    pub codec_path: CodecPath,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ChromeTraceEvent {
    pub name: String,
    pub cat: String,
    pub ph: String,
    pub ts: u64,
    pub pid: u32,
    pub tid: u32,
    pub args: BTreeMap<String, serde_json::Value>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TraceDocument {
    #[serde(rename = "traceEvents")]
    pub trace_events: Vec<ChromeTraceEvent>,
}

impl TraceDocument {
    /// Constructs paired Chrome trace spans for every benchmark pipeline stage.
    ///
    /// # Panics
    ///
    /// Panics only if the fixed stage-count index cannot fit in a `u64` trace
    /// timestamp.
    #[must_use]
    pub fn full_pipeline(context: &TraceContext) -> Self {
        let mut trace_events = Vec::with_capacity(REQUIRED_TRACE_STAGES.len() * 2);
        for (index, stage) in REQUIRED_TRACE_STAGES.iter().enumerate() {
            let start = u64::try_from(index).expect("stage fits u64");
            let name = trace_stage_name(*stage).to_owned();
            let args = trace_args(context, *stage);
            trace_events.push(ChromeTraceEvent {
                name: name.clone(),
                cat: "stitch.benchmark".to_owned(),
                ph: "B".to_owned(),
                ts: start,
                pid: 1,
                tid: 1,
                args,
            });
            trace_events.push(ChromeTraceEvent {
                name,
                cat: "stitch.benchmark".to_owned(),
                ph: "E".to_owned(),
                ts: start + 1,
                pid: 1,
                tid: 1,
                args: BTreeMap::new(),
            });
        }
        Self { trace_events }
    }

    /// Validates Chrome trace JSON shape, required stages, and disclosure fields.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed events or incomplete metadata.
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.trace_events.is_empty() {
            return Err(ValidationError::InvalidTrace("traceEvents is empty"));
        }
        let encoded = serde_json::to_vec(self)
            .map_err(|_| ValidationError::InvalidTrace("trace cannot serialize"))?;
        let decoded: serde_json::Value = serde_json::from_slice(&encoded)
            .map_err(|_| ValidationError::InvalidTrace("trace is not JSON"))?;
        if decoded
            .get("traceEvents")
            .and_then(serde_json::Value::as_array)
            .is_none()
        {
            return Err(ValidationError::InvalidTrace("missing traceEvents"));
        }
        let mut begun = BTreeSet::new();
        let mut ended = BTreeSet::new();
        for event in &self.trace_events {
            if event.cat != "stitch.benchmark" || !(event.ph == "B" || event.ph == "E") {
                return Err(ValidationError::InvalidTrace("invalid Chrome trace event"));
            }
            if event.ph == "B" {
                for key in [
                    "trace_revision",
                    "project_revision",
                    "semantic_digest",
                    "execution_digest",
                    "mix_digest",
                    "quality",
                    "generation",
                    "canonical_time",
                    "adapter_tick",
                    "adapter_timescale",
                    "adapter_rounding",
                    "adapter_rounding_error",
                    "adapter_error",
                    "queue_items",
                    "queue_bytes",
                    "resource_tokens",
                    "process_rss_bytes",
                    "gpu_memory_bytes",
                    "codec_path",
                ] {
                    if !event.args.contains_key(key) {
                        return Err(ValidationError::InvalidTrace("missing trace metadata"));
                    }
                }
                if !valid_unsigned_rational(event.args.get("canonical_time"))
                    || !valid_signed_rational(event.args.get("adapter_rounding_error"))
                    || event
                        .args
                        .get("adapter_timescale")
                        .and_then(serde_json::Value::as_u64)
                        .is_none_or(|timescale| timescale == 0)
                {
                    return Err(ValidationError::InvalidTrace(
                        "invalid exact timing metadata",
                    ));
                }
                begun.insert(event.name.as_str());
            } else {
                ended.insert(event.name.as_str());
            }
        }
        for stage in REQUIRED_TRACE_STAGES {
            let name = trace_stage_name(stage);
            if !begun.contains(name) || !ended.contains(name) {
                return Err(ValidationError::InvalidTrace("missing required stage"));
            }
        }
        Ok(())
    }

    /// Serializes a portable Chrome/Perfetto trace document.
    ///
    /// # Errors
    ///
    /// Returns an error when JSON serialization fails.
    pub fn deterministic_json(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec_pretty(self)
    }
}

fn valid_unsigned_rational(value: Option<&serde_json::Value>) -> bool {
    value
        .and_then(serde_json::Value::as_object)
        .is_some_and(|rational| {
            rational
                .get("numerator")
                .and_then(serde_json::Value::as_u64)
                .is_some()
                && rational
                    .get("denominator")
                    .and_then(serde_json::Value::as_u64)
                    .is_some_and(|denominator| denominator != 0)
        })
}

fn valid_signed_rational(value: Option<&serde_json::Value>) -> bool {
    value
        .and_then(serde_json::Value::as_object)
        .is_some_and(|rational| {
            rational
                .get("numerator")
                .and_then(serde_json::Value::as_i64)
                .is_some()
                && rational
                    .get("denominator")
                    .and_then(serde_json::Value::as_u64)
                    .is_some_and(|denominator| denominator != 0)
        })
}

fn trace_stage_name(stage: TraceStage) -> &'static str {
    match stage {
        TraceStage::Probe => "probe",
        TraceStage::Io => "i_o",
        TraceStage::Demux => "demux",
        TraceStage::Index => "index",
        TraceStage::Decode => "decode",
        TraceStage::Upload => "upload",
        TraceStage::Node => "node",
        TraceStage::Composite => "composite",
        TraceStage::Audio => "audio",
        TraceStage::Present => "present",
        TraceStage::Encode => "encode",
        TraceStage::Mux => "mux",
        TraceStage::Cache => "cache",
        TraceStage::Cancel => "cancel",
        TraceStage::Error => "error",
        TraceStage::Finalize => "finalize",
    }
}

fn trace_args(context: &TraceContext, stage: TraceStage) -> BTreeMap<String, serde_json::Value> {
    let mut args = BTreeMap::new();
    args.insert(
        "stage".to_owned(),
        serde_json::json!(trace_stage_name(stage)),
    );
    args.insert(
        "trace_revision".to_owned(),
        serde_json::json!(context.trace_revision),
    );
    args.insert(
        "project_revision".to_owned(),
        serde_json::json!(context.project_revision),
    );
    args.insert(
        "semantic_digest".to_owned(),
        serde_json::json!(context.semantic_digest),
    );
    args.insert(
        "execution_digest".to_owned(),
        serde_json::json!(context.execution_digest),
    );
    args.insert(
        "mix_digest".to_owned(),
        serde_json::json!(context.mix_digest),
    );
    args.insert("quality".to_owned(), serde_json::json!(context.quality));
    args.insert(
        "generation".to_owned(),
        serde_json::json!(context.generation),
    );
    args.insert(
        "canonical_time".to_owned(),
        serde_json::json!(context.canonical_time),
    );
    args.insert(
        "adapter_tick".to_owned(),
        serde_json::json!(context.adapter_timing.tick),
    );
    args.insert(
        "adapter_timescale".to_owned(),
        serde_json::json!(context.adapter_timing.timescale),
    );
    args.insert(
        "adapter_rounding".to_owned(),
        serde_json::json!(context.adapter_timing.rounding),
    );
    args.insert(
        "adapter_rounding_error".to_owned(),
        serde_json::json!(context.adapter_timing.rounding_error),
    );
    args.insert(
        "adapter_error".to_owned(),
        serde_json::json!(context.adapter_error),
    );
    args.insert(
        "queue_items".to_owned(),
        serde_json::json!(context.queue_items),
    );
    args.insert(
        "queue_bytes".to_owned(),
        serde_json::json!(context.queue_bytes),
    );
    args.insert(
        "resource_tokens".to_owned(),
        serde_json::json!(context.resource_tokens),
    );
    args.insert(
        "process_rss_bytes".to_owned(),
        serde_json::json!(context.process_rss_bytes),
    );
    args.insert(
        "gpu_memory_bytes".to_owned(),
        serde_json::json!(context.gpu_memory_bytes),
    );
    args.insert(
        "codec_path".to_owned(),
        serde_json::json!(context.codec_path),
    );
    args
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RenderManifest {
    pub project_revision: String,
    pub semantic_digest: String,
    pub execution_digest: String,
    pub mix_digest: String,
    pub quality: String,
    pub generation: u64,
    pub canonical_time: ExactRational,
    pub output_sha256: String,
    pub adapter_error: Option<String>,
}

impl RenderManifest {
    /// Validates the immutable project/render identity fields.
    ///
    /// # Errors
    ///
    /// Returns an error when a required render-manifest field is blank.
    pub fn validate(&self) -> Result<(), ValidationError> {
        for (field, value) in [
            ("project_revision", &self.project_revision),
            ("semantic_digest", &self.semantic_digest),
            ("execution_digest", &self.execution_digest),
            ("mix_digest", &self.mix_digest),
            ("quality", &self.quality),
            ("output_sha256", &self.output_sha256),
        ] {
            if value.trim().is_empty() {
                return Err(ValidationError::InvalidRenderManifest(field));
            }
        }
        if self.canonical_time.denominator == 0 {
            return Err(ValidationError::InvalidRenderManifest(
                "canonical_time.denominator",
            ));
        }
        Ok(())
    }

    /// Serializes the render manifest with stable ordering.
    ///
    /// # Errors
    ///
    /// Returns an error when JSON serialization fails.
    pub fn deterministic_json(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec_pretty(self)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ComparisonResult {
    pub candidate_correctness_failed: bool,
    pub correctness_worsened: bool,
    pub flags: Vec<ComparisonFlag>,
    pub by_temperature: Vec<TemperatureComparison>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TemperatureComparison {
    pub temperature: CacheTemperature,
    pub valid_pairs: usize,
    pub same_machine_fingerprint: Option<String>,
    pub candidate_median_ns: Option<u64>,
    pub baseline_median_ns: Option<u64>,
    pub paired_ratios: Vec<PairedRatio>,
    pub median_ratio: Option<MedianRatio>,
    pub candidate_over_five_percent_slower: bool,
    pub flags: Vec<ComparisonFlag>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PairedRatio {
    pub pair: u32,
    pub candidate_ns: u64,
    pub baseline_ns: u64,
    pub ratio: ExactRational,
}

/// The exact middle ratio(s). For an even count, the statistical median is the
/// arithmetic mean of `lower` and `upper`; keeping both avoids lossy decimals.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MedianRatio {
    pub lower: ExactRational,
    pub upper: ExactRational,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonFlag {
    InsufficientSameMachinePairs,
    MixedMachineEvidence,
    IncompatiblePairingIdentity,
    CandidateCorrectnessFailed,
    CorrectnessWorsened,
    CandidateOverFivePercentSlower,
}

/// Compares cold and warm matched pairs independently. Each temperature needs at
/// least ten valid same-machine pairs, and each timing gate is calculated from the
/// median of the paired candidate/baseline ratios rather than pooled medians.
#[must_use]
pub fn compare(bundle: &BenchmarkBundle) -> ComparisonResult {
    let mut by_pair: BTreeMap<(u32, CacheTemperature), (&RawRepetition, &RawRepetition)> =
        BTreeMap::new();
    let mut candidate_by_key = BTreeMap::new();
    let mut baseline_by_key = BTreeMap::new();
    for run in &bundle.raw_repetitions {
        let key = (run.schedule.pair, run.schedule.temperature);
        match run.schedule.variant {
            Variant::Candidate => {
                candidate_by_key.insert(key, run);
            }
            Variant::Baseline => {
                baseline_by_key.insert(key, run);
            }
        }
    }
    for (key, candidate) in candidate_by_key {
        if let Some(baseline) = baseline_by_key.get(&key) {
            by_pair.insert(key, (candidate, *baseline));
        }
    }

    let mut candidate_correctness_failed = false;
    let mut correctness_worsened = false;
    let by_temperature = [CacheTemperature::Cold, CacheTemperature::Warm]
        .into_iter()
        .map(|temperature| {
            compare_temperature(
                &by_pair,
                temperature,
                &mut candidate_correctness_failed,
                &mut correctness_worsened,
            )
        })
        .collect();
    let mut flags = Vec::new();
    if candidate_correctness_failed {
        flags.push(ComparisonFlag::CandidateCorrectnessFailed);
    }
    if correctness_worsened {
        flags.push(ComparisonFlag::CorrectnessWorsened);
    }
    ComparisonResult {
        candidate_correctness_failed,
        correctness_worsened,
        flags,
        by_temperature,
    }
}

fn compare_temperature(
    by_pair: &BTreeMap<(u32, CacheTemperature), (&RawRepetition, &RawRepetition)>,
    temperature: CacheTemperature,
    candidate_correctness_failed: &mut bool,
    correctness_worsened: &mut bool,
) -> TemperatureComparison {
    let mut flags = Vec::new();
    let mut same_machine = None;
    let mut same_pairing_identity = None;
    let mut mixed_machine = false;
    let mut incompatible_identity = false;
    let mut candidates = Vec::new();
    let mut baselines = Vec::new();
    let mut paired_ratios = Vec::new();
    for ((pair, pair_temperature), (candidate, baseline)) in by_pair {
        if *pair_temperature != temperature {
            continue;
        }
        *candidate_correctness_failed |=
            !candidate.correctness_passed || candidate.adapter_error.is_some();
        *correctness_worsened |= baseline.correctness_passed
            && baseline.adapter_error.is_none()
            && (!candidate.correctness_passed || candidate.adapter_error.is_some());
        let candidate_machine = candidate.disclosure.machine.fingerprint();
        let candidate_identity = candidate.pairing_identity();
        if candidate_machine != baseline.disclosure.machine.fingerprint() {
            mixed_machine = true;
            incompatible_identity = true;
            continue;
        }
        if candidate_identity != baseline.pairing_identity() {
            incompatible_identity = true;
            continue;
        }
        if let Some(expected) = &same_machine {
            if expected != &candidate_machine {
                mixed_machine = true;
                continue;
            }
        } else {
            same_machine = Some(candidate_machine);
        }
        if let Some(expected) = &same_pairing_identity {
            if expected != &candidate_identity {
                incompatible_identity = true;
                continue;
            }
        } else {
            same_pairing_identity = Some(candidate_identity);
        }
        record_valid_pair(
            *pair,
            candidate,
            baseline,
            &mut candidates,
            &mut baselines,
            &mut paired_ratios,
        );
    }
    if mixed_machine {
        flags.push(ComparisonFlag::MixedMachineEvidence);
    }
    if incompatible_identity {
        flags.push(ComparisonFlag::IncompatiblePairingIdentity);
    }
    let valid_pairs = paired_ratios.len();
    let (candidate_median_ns, baseline_median_ns, median_ratio, slower) = compare_timings(
        valid_pairs,
        &mut candidates,
        &mut baselines,
        &mut paired_ratios,
        &mut flags,
    );
    TemperatureComparison {
        temperature,
        valid_pairs,
        same_machine_fingerprint: same_machine,
        candidate_median_ns,
        baseline_median_ns,
        paired_ratios,
        median_ratio,
        candidate_over_five_percent_slower: slower,
        flags,
    }
}

fn record_valid_pair(
    pair: u32,
    candidate: &RawRepetition,
    baseline: &RawRepetition,
    candidates: &mut Vec<u64>,
    baselines: &mut Vec<u64>,
    paired_ratios: &mut Vec<PairedRatio>,
) {
    if let (Some(candidate_ns), Some(baseline_ns)) = (candidate.elapsed_ns, baseline.elapsed_ns)
        && candidate.is_valid_measurement()
        && baseline.is_valid_measurement()
        && baseline_ns != 0
    {
        candidates.push(candidate_ns);
        baselines.push(baseline_ns);
        paired_ratios.push(PairedRatio {
            pair,
            candidate_ns,
            baseline_ns,
            ratio: ExactRational {
                numerator: candidate_ns,
                denominator: baseline_ns,
            },
        });
    }
}

fn compare_timings(
    valid_pairs: usize,
    candidates: &mut [u64],
    baselines: &mut [u64],
    paired_ratios: &mut [PairedRatio],
    flags: &mut Vec<ComparisonFlag>,
) -> (Option<u64>, Option<u64>, Option<MedianRatio>, bool) {
    if valid_pairs < 10 {
        flags.push(ComparisonFlag::InsufficientSameMachinePairs);
        return (None, None, None, false);
    }
    let candidate_median_ns = median(candidates);
    let baseline_median_ns = median(baselines);
    let median_ratio = median_ratio(paired_ratios);
    let slower = median_ratio_exceeds(&median_ratio, 21, 20);
    if slower {
        flags.push(ComparisonFlag::CandidateOverFivePercentSlower);
    }
    (
        Some(candidate_median_ns),
        Some(baseline_median_ns),
        Some(median_ratio),
        slower,
    )
}

fn median_ratio(ratios: &mut [PairedRatio]) -> MedianRatio {
    ratios.sort_by(|left, right| compare_rationals(&left.ratio, &right.ratio));
    let midpoint = ratios.len() / 2;
    let lower = ratios[(ratios.len() - 1) / 2].ratio.clone();
    let upper = ratios[midpoint].ratio.clone();
    MedianRatio { lower, upper }
}

fn compare_rationals(left: &ExactRational, right: &ExactRational) -> std::cmp::Ordering {
    u128::from(left.numerator)
        .saturating_mul(u128::from(right.denominator))
        .cmp(&u128::from(right.numerator).saturating_mul(u128::from(left.denominator)))
}

fn median_ratio_exceeds(median: &MedianRatio, numerator: u64, denominator: u64) -> bool {
    let sum_numerators = u128::from(median.lower.numerator)
        .saturating_mul(u128::from(median.upper.denominator))
        .saturating_add(
            u128::from(median.upper.numerator).saturating_mul(u128::from(median.lower.denominator)),
        );
    let left = u128::from(denominator).saturating_mul(sum_numerators);
    let right = u128::from(numerator)
        .saturating_mul(2)
        .saturating_mul(u128::from(median.lower.denominator))
        .saturating_mul(u128::from(median.upper.denominator));
    left > right
}

fn median(values: &mut [u64]) -> u64 {
    values.sort_unstable();
    let midpoint = values.len() / 2;
    if values.len() % 2 == 1 {
        values[midpoint]
    } else {
        values[midpoint - 1].midpoint(values[midpoint])
    }
}

/// Computes the SHA-256 digest of a serializable benchmark value.
///
/// # Panics
///
/// Panics when a custom [`Serialize`] implementation returns an error.
#[must_use]
pub fn stable_digest<T: Serialize>(value: &T) -> String {
    let encoded = serde_json::to_vec(value).expect("serializable benchmark value");
    let digest = Sha256::digest(encoded);
    format!("{digest:x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeded_interleaving_is_deterministic_and_complete() {
        let first = seeded_interleaving(7, 10);
        assert_eq!(first, seeded_interleaving(7, 10));
        assert_ne!(first, seeded_interleaving(8, 10));
        assert_eq!(first.len(), 40);
        validate_schedule(&first).expect("every pair and temperature is present once");
    }

    #[test]
    fn bundle_rejects_missing_disclosure() {
        let (mut bundle, _) = valid_bundle(10, 100, 100);
        bundle.raw_repetitions[0].disclosure.codec.profile.clear();
        assert_eq!(
            bundle.validate(),
            Err(ValidationError::MissingDisclosure("codec.profile"))
        );
    }

    #[test]
    fn bundle_rejects_absent_or_zero_structured_disclosure_fields() {
        let (mut missing_os_build, _) = valid_bundle(10, 100, 100);
        missing_os_build.raw_repetitions[0]
            .disclosure
            .machine
            .os
            .build
            .clear();
        assert_eq!(
            missing_os_build.validate(),
            Err(ValidationError::MissingDisclosure("machine.os.build"))
        );

        let (mut zero_gpu_budget, _) = valid_bundle(10, 100, 100);
        zero_gpu_budget.raw_repetitions[0]
            .disclosure
            .machine
            .gpu
            .vram_or_unified_budget_bytes = 0;
        assert_eq!(
            zero_gpu_budget.validate(),
            Err(ValidationError::MissingDisclosure(
                "machine.gpu.vram_or_unified_budget_bytes"
            ))
        );
    }

    #[test]
    fn bundle_rejects_repetition_with_a_mismatched_sequence_number() {
        let (mut bundle, _) = valid_bundle(10, 100, 100);
        bundle.raw_repetitions[0].schedule.sequence = 99_999;
        assert_eq!(
            bundle.validate(),
            Err(ValidationError::ScheduleMismatch),
            "a repetition must preserve the exact scheduled sequence, not only its pair identity"
        );
    }

    #[test]
    fn bundle_never_drops_failures() {
        let (mut bundle, _) = valid_bundle(10, 100, 100);
        let candidate = bundle
            .raw_repetitions
            .iter_mut()
            .find(|run| run.schedule.variant == Variant::Candidate)
            .expect("schedule has a candidate");
        candidate.correctness_passed = false;
        assert_eq!(bundle.failures.len(), 0);
        assert_eq!(
            bundle.validate(),
            Err(ValidationError::FailureRetentionMismatch),
            "changing a raw result without recording the failure must be rejected"
        );
        bundle = BenchmarkBundle::new(
            bundle.seed,
            bundle.schedule,
            bundle.raw_repetitions,
            bundle.trace,
            bundle.render_manifest,
        );
        assert_eq!(bundle.failures.len(), 1);
        bundle
            .validate()
            .expect("failure is retained in raw and summary evidence");
    }

    #[test]
    fn trace_serializes_as_valid_chrome_trace_json() {
        let context = trace_context();
        let trace = TraceDocument::full_pipeline(&context);
        trace
            .validate()
            .expect("all required pipeline stages have B/E spans");
        let json: serde_json::Value =
            serde_json::from_slice(&trace.deterministic_json().expect("trace JSON"))
                .expect("valid JSON");
        assert_eq!(json["traceEvents"].as_array().map(Vec::len), Some(32));
        let args = &json["traceEvents"][0]["args"];
        assert_eq!(args["canonical_time"]["numerator"], 1);
        assert_eq!(args["canonical_time"]["denominator"], 48_000);
        assert_eq!(args["adapter_tick"], 1);
        assert_eq!(args["adapter_timescale"], 48_000);
        assert!(args.get("canonical_time_ns").is_none());
    }

    #[test]
    fn comparison_flags_strictly_more_than_five_percent() {
        let (exactly_five, _) = valid_bundle(10, 105, 100);
        let result = compare(&exactly_five);
        assert!(result.by_temperature.iter().all(|comparison| {
            comparison.valid_pairs == 10 && !comparison.candidate_over_five_percent_slower
        }));

        let (over_five, _) = valid_bundle(10, 106, 100);
        let result = compare(&over_five);
        assert!(
            result
                .by_temperature
                .iter()
                .all(|comparison| comparison.candidate_over_five_percent_slower)
        );
    }

    #[test]
    fn comparison_requires_ten_valid_same_machine_pairs_and_flags_correctness() {
        let (mut bundle, _) = valid_bundle(5, 100, 100);
        let candidate = bundle
            .raw_repetitions
            .iter_mut()
            .find(|run| run.schedule.variant == Variant::Candidate)
            .expect("schedule has a candidate");
        candidate.correctness_passed = false;
        bundle = BenchmarkBundle::new(
            bundle.seed,
            bundle.schedule,
            bundle.raw_repetitions,
            bundle.trace,
            bundle.render_manifest,
        );
        let result = compare(&bundle);
        assert!(
            result
                .by_temperature
                .iter()
                .all(|comparison| comparison.valid_pairs < 10)
        );
        assert!(result.correctness_worsened);
    }

    #[test]
    fn candidate_failure_is_reported_even_when_baseline_also_fails() {
        let (mut bundle, _) = valid_bundle(10, 100, 100);
        for run in &mut bundle.raw_repetitions {
            if run.schedule.temperature == CacheTemperature::Cold && run.schedule.pair == 0 {
                run.correctness_passed = false;
            }
        }
        let result = compare(&rebuild(bundle));
        assert!(result.candidate_correctness_failed);
        assert!(!result.correctness_worsened);
        assert!(
            result
                .flags
                .contains(&ComparisonFlag::CandidateCorrectnessFailed)
        );
        assert!(!result.flags.contains(&ComparisonFlag::CorrectnessWorsened));
    }

    #[test]
    fn comparison_does_not_pool_cold_and_warm_results() {
        let (mut bundle, _) = valid_bundle(10, 100, 100);
        for run in &mut bundle.raw_repetitions {
            run.elapsed_ns = Some(match (run.schedule.temperature, run.schedule.variant) {
                (CacheTemperature::Cold, Variant::Candidate) => 106,
                (CacheTemperature::Warm, Variant::Candidate) => 50,
                (_, Variant::Baseline) => 100,
            });
        }
        bundle = rebuild(bundle);
        let result = compare(&bundle);
        let cold = temperature(&result, CacheTemperature::Cold);
        let warm = temperature(&result, CacheTemperature::Warm);
        assert!(cold.candidate_over_five_percent_slower);
        assert!(!warm.candidate_over_five_percent_slower);
    }

    #[test]
    fn comparison_uses_paired_ratio_median_not_ratio_of_medians() {
        let (mut bundle, _) = valid_bundle(10, 1, 1);
        for run in &mut bundle.raw_repetitions {
            let (candidate_ns, baseline_ns) = match run.schedule.pair {
                0..=2 => (2, 1),
                3..=5 => (2_000, 1_000),
                _ => (1, 500),
            };
            run.elapsed_ns = Some(match run.schedule.variant {
                Variant::Candidate => candidate_ns,
                Variant::Baseline => baseline_ns,
            });
        }
        bundle = rebuild(bundle);
        let result = compare(&bundle);
        for comparison in &result.by_temperature {
            assert!(comparison.candidate_over_five_percent_slower);
            assert_eq!(comparison.candidate_median_ns, Some(2));
            assert_eq!(comparison.baseline_median_ns, Some(500));
            assert_eq!(comparison.paired_ratios.len(), 10);
        }
    }

    #[test]
    fn comparison_rejects_codec_path_mismatch() {
        let (mut bundle, _) = valid_bundle(10, 100, 100);
        let candidate = bundle
            .raw_repetitions
            .iter_mut()
            .find(|run| {
                run.schedule.temperature == CacheTemperature::Cold
                    && run.schedule.variant == Variant::Candidate
            })
            .expect("cold candidate exists");
        candidate.disclosure.codec.decode_path = CodecPath::Software;
        bundle = rebuild(bundle);
        let cold = temperature(&compare(&bundle), CacheTemperature::Cold).clone();
        assert_eq!(cold.valid_pairs, 9);
        assert!(
            cold.flags
                .contains(&ComparisonFlag::IncompatiblePairingIdentity)
        );
    }

    #[test]
    fn comparison_rejects_workload_digest_mismatch() {
        let (mut bundle, _) = valid_bundle(10, 100, 100);
        let candidate = bundle
            .raw_repetitions
            .iter_mut()
            .find(|run| {
                run.schedule.temperature == CacheTemperature::Cold
                    && run.schedule.variant == Variant::Candidate
            })
            .expect("cold candidate exists");
        candidate.disclosure.artifacts.fixture_sha256 = "other-fixture".to_owned();
        bundle = rebuild(bundle);
        let cold = temperature(&compare(&bundle), CacheTemperature::Cold).clone();
        assert_eq!(cold.valid_pairs, 9);
        assert!(
            cold.flags
                .contains(&ComparisonFlag::IncompatiblePairingIdentity)
        );
    }

    #[test]
    fn comparison_rejects_machine_audio_and_runtime_mismatches() {
        assert_incompatible_pairing(|run| {
            run.disclosure.machine.os.build = "different-build".to_owned();
        });
        assert_incompatible_pairing(|run| {
            run.disclosure.audio.driver = "different-driver".to_owned();
        });
        assert_incompatible_pairing(|run| {
            run.disclosure.runtime.cache_state = "warm".to_owned();
        });
    }

    #[test]
    fn comparison_allows_commit_and_render_output_differences() {
        let (mut bundle, _) = valid_bundle(10, 100, 100);
        let candidate = bundle
            .raw_repetitions
            .iter_mut()
            .find(|run| {
                run.schedule.temperature == CacheTemperature::Cold
                    && run.schedule.variant == Variant::Candidate
            })
            .expect("cold candidate exists");
        candidate.disclosure.source.commit = "newer-commit".to_owned();
        candidate.disclosure.artifacts.render_sha256 = "new-render".to_owned();
        bundle = rebuild(bundle);
        let cold = temperature(&compare(&bundle), CacheTemperature::Cold).clone();
        assert_eq!(cold.valid_pairs, 10);
        assert!(
            !cold
                .flags
                .contains(&ComparisonFlag::IncompatiblePairingIdentity)
        );
    }

    #[test]
    fn schedule_requires_contiguous_sequence_numbers() {
        let mut schedule = seeded_interleaving(7, 1);
        schedule[0].sequence = 99;
        assert_eq!(
            validate_schedule(&schedule),
            Err(ValidationError::ScheduleMismatch)
        );
    }

    fn valid_bundle(
        pairs: u32,
        candidate_ns: u64,
        baseline_ns: u64,
    ) -> (BenchmarkBundle, Vec<RawRepetition>) {
        let schedule = seeded_interleaving(99, pairs);
        let runs: Vec<_> = schedule
            .iter()
            .cloned()
            .map(|schedule| RawRepetition {
                elapsed_ns: Some(match schedule.variant {
                    Variant::Candidate => candidate_ns,
                    Variant::Baseline => baseline_ns,
                }),
                correctness_passed: true,
                adapter_error: None,
                disclosure: disclosure(),
                resource_observation: ResourceObservation {
                    process_rss_bytes: 10,
                    gpu_memory_bytes: Some(2),
                    queue_items: 0,
                    queue_bytes: 0,
                    resource_tokens: BTreeMap::from([("decode".to_owned(), 1)]),
                    codec_path: CodecPath::Hardware,
                },
                schedule,
            })
            .collect();
        let trace = TraceDocument::full_pipeline(&trace_context());
        let manifest = RenderManifest {
            project_revision: "project-r1".to_owned(),
            semantic_digest: "semantic".to_owned(),
            execution_digest: "execution".to_owned(),
            mix_digest: "mix".to_owned(),
            quality: "preview".to_owned(),
            generation: 1,
            canonical_time: ExactRational {
                numerator: 1,
                denominator: 48_000,
            },
            output_sha256: "output".to_owned(),
            adapter_error: None,
        };
        let bundle = BenchmarkBundle::new(99, schedule, runs.clone(), trace, manifest);
        (bundle, runs)
    }

    fn rebuild(bundle: BenchmarkBundle) -> BenchmarkBundle {
        BenchmarkBundle::new(
            bundle.seed,
            bundle.schedule,
            bundle.raw_repetitions,
            bundle.trace,
            bundle.render_manifest,
        )
    }

    fn temperature(
        result: &ComparisonResult,
        temperature: CacheTemperature,
    ) -> &TemperatureComparison {
        result
            .by_temperature
            .iter()
            .find(|comparison| comparison.temperature == temperature)
            .expect("comparison contains both temperatures")
    }

    fn assert_incompatible_pairing(mutator: impl FnOnce(&mut RawRepetition)) {
        let (mut bundle, _) = valid_bundle(10, 100, 100);
        let candidate = bundle
            .raw_repetitions
            .iter_mut()
            .find(|run| {
                run.schedule.temperature == CacheTemperature::Cold
                    && run.schedule.variant == Variant::Candidate
            })
            .expect("cold candidate exists");
        mutator(candidate);
        let cold = temperature(&compare(&rebuild(bundle)), CacheTemperature::Cold).clone();
        assert_eq!(cold.valid_pairs, 9);
        assert!(
            cold.flags
                .contains(&ComparisonFlag::IncompatiblePairingIdentity)
        );
    }

    fn disclosure() -> DisclosureBundle {
        DisclosureBundle {
            source: SourceDisclosure {
                commit: "abc".to_owned(),
                dirty: false,
                build_flags: vec!["--release".to_owned()],
                lockfile_sha256: "lock".to_owned(),
                sbom_sha256: "sbom".to_owned(),
            },
            artifacts: ArtifactDigests {
                fixture_sha256: "fixture".to_owned(),
                project_sha256: "project".to_owned(),
                proxy_sha256: "proxy".to_owned(),
                render_sha256: "render".to_owned(),
                oracle_sha256: "oracle".to_owned(),
            },
            machine: MachineDisclosure {
                os: OsDisclosure {
                    name: "macOS".to_owned(),
                    build: "24A1".to_owned(),
                    security_updates: "2026-07".to_owned(),
                    graphics_updates: "metal-1".to_owned(),
                },
                cpu: CpuDisclosure {
                    model: "CPU".to_owned(),
                    physical_cores: 8,
                    logical_cores: 8,
                    scheduler: "default".to_owned(),
                },
                memory: MemoryDisclosure {
                    bytes: 16 * 1024 * 1024 * 1024,
                    speed_mhz: 6_400,
                },
                gpu: GpuDisclosure {
                    model: "GPU".to_owned(),
                    driver: "system".to_owned(),
                    vram_or_unified_budget_bytes: 8 * 1024 * 1024 * 1024,
                    metal_feature_set: "macOS_GPUFamily2_v1".to_owned(),
                    d3d_feature_level: "not_applicable".to_owned(),
                },
                storage: StorageDisclosure {
                    model: "SSD".to_owned(),
                    interface: "NVMe".to_owned(),
                    filesystem: "APFS".to_owned(),
                    free_bytes: 100 * 1024 * 1024 * 1024,
                    project_volume: "project".to_owned(),
                    cache_volume: "cache".to_owned(),
                },
                display: DisplayDisclosure {
                    width_px: 2_560,
                    height_px: 1_600,
                    refresh_millihz: 60_000,
                    dpi_milli: 227_000,
                    qt_backend: "cocoa".to_owned(),
                },
                power: "AC".to_owned(),
                thermal: "nominal".to_owned(),
            },
            codec: CodecDisclosure {
                video_codec: "h264".to_owned(),
                profile: "high".to_owned(),
                level: "5.1".to_owned(),
                sample_entry: "avc1".to_owned(),
                configuration: "avcC".to_owned(),
                decode_path: CodecPath::Hardware,
                encode_path: CodecPath::Hardware,
                surface_format: "nv12".to_owned(),
            },
            audio: AudioDisclosure {
                device: "default".to_owned(),
                driver: "coreaudio".to_owned(),
                sample_rate_hz: 48_000,
                buffer_frames: 256,
                offset_frames: 0,
                mode: "exclusive".to_owned(),
            },
            runtime: RuntimeDisclosure {
                cache_state: "cold".to_owned(),
                background_load: "idle".to_owned(),
                network_state: "offline".to_owned(),
            },
            references: EvidenceReferences {
                trace: "trace.json".to_owned(),
                render_manifest: "render.json".to_owned(),
                failure_bundle: "failures.json".to_owned(),
            },
        }
    }

    fn trace_context() -> TraceContext {
        TraceContext {
            trace_revision: "trace-r1".to_owned(),
            project_revision: "project-r1".to_owned(),
            semantic_digest: "semantic".to_owned(),
            execution_digest: "execution".to_owned(),
            mix_digest: "mix".to_owned(),
            quality: "preview".to_owned(),
            generation: 1,
            canonical_time: ExactRational {
                numerator: 1,
                denominator: 48_000,
            },
            adapter_timing: AdapterTiming {
                tick: 1,
                timescale: 48_000,
                rounding: RoundingMode::NearestTiesToEven,
                rounding_error: ExactSignedRational {
                    numerator: 0,
                    denominator: 1,
                },
            },
            adapter_error: None,
            queue_items: 0,
            queue_bytes: 0,
            resource_tokens: BTreeMap::from([("decode".to_owned(), 1)]),
            process_rss_bytes: 1,
            gpu_memory_bytes: Some(1),
            codec_path: CodecPath::Hardware,
        }
    }
}
