//! Versioned, platform-neutral contracts shared by the editor core and storage.

#![forbid(unsafe_code)]

use core::cmp::Ordering;
use core::fmt;
use std::collections::BTreeSet;

use im::{OrdMap, Vector};
use sha2::{Digest, Sha256};

/// First stable version of the canonical project-contract encoding.
pub const CONTRACT_SCHEMA_VERSION: u32 = 1;

/// A checked, normalized rational value used for all canonical time values.
///
/// Denominators are always positive. Zero is represented only as `0/1`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Rational {
    numerator: i64,
    denominator: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimeError {
    ZeroDenominator,
    Overflow,
    InvalidRange,
    InvalidUnit,
    NonMonotonicAdaptedTicks,
}

impl fmt::Display for TimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroDenominator => formatter.write_str("rational denominator must not be zero"),
            Self::Overflow => formatter.write_str("rational operation overflowed"),
            Self::InvalidRange => {
                formatter.write_str("time range must have start strictly before end")
            }
            Self::InvalidUnit => formatter.write_str("adapter unit must have a positive tick rate"),
            Self::NonMonotonicAdaptedTicks => {
                formatter.write_str("adapter rounded distinct timestamps to non-monotonic ticks")
            }
        }
    }
}

impl std::error::Error for TimeError {}

impl Rational {
    /// Creates a normalized rational without silently rounding or overflowing.
    ///
    /// # Errors
    ///
    /// Returns [`TimeError`] when the denominator is zero or normalization
    /// cannot be represented by the canonical `i64` fields.
    pub fn new(numerator: i64, denominator: i64) -> Result<Self, TimeError> {
        Self::from_i128(i128::from(numerator), i128::from(denominator))
    }

    #[must_use]
    pub const fn numerator(self) -> i64 {
        self.numerator
    }

    #[must_use]
    pub const fn denominator(self) -> i64 {
        self.denominator
    }

    /// Adds two rationals without rounding.
    ///
    /// # Errors
    ///
    /// Returns [`TimeError::Overflow`] when the normalized result is not
    /// representable by the canonical fields.
    pub fn checked_add(self, other: Self) -> Result<Self, TimeError> {
        let numerator = i128::from(self.numerator) * i128::from(other.denominator)
            + i128::from(other.numerator) * i128::from(self.denominator);
        let denominator = i128::from(self.denominator) * i128::from(other.denominator);
        Self::from_i128(numerator, denominator)
    }

    /// Subtracts two rationals without rounding.
    ///
    /// # Errors
    ///
    /// Returns [`TimeError::Overflow`] when the normalized result is not
    /// representable by the canonical fields.
    pub fn checked_sub(self, other: Self) -> Result<Self, TimeError> {
        let numerator = i128::from(self.numerator) * i128::from(other.denominator)
            - i128::from(other.numerator) * i128::from(self.denominator);
        let denominator = i128::from(self.denominator) * i128::from(other.denominator);
        Self::from_i128(numerator, denominator)
    }

    /// Multiplies two rationals without rounding.
    ///
    /// # Errors
    ///
    /// Returns [`TimeError::Overflow`] when the normalized result is not
    /// representable by the canonical fields.
    pub fn checked_mul(self, other: Self) -> Result<Self, TimeError> {
        Self::from_i128(
            i128::from(self.numerator) * i128::from(other.numerator),
            i128::from(self.denominator) * i128::from(other.denominator),
        )
    }

    /// Returns the reciprocal of a nonzero rational.
    ///
    /// # Errors
    ///
    /// Returns [`TimeError`] for a zero numerator or an unrepresentable result.
    pub fn checked_reciprocal(self) -> Result<Self, TimeError> {
        Self::new(self.denominator, self.numerator)
    }

    /// Big-endian canonical encoding used by the project model's canonical bytes.
    #[must_use]
    pub fn canonical_bytes(self) -> [u8; 16] {
        let mut bytes = [0; 16];
        bytes[..8].copy_from_slice(&self.numerator.to_be_bytes());
        bytes[8..].copy_from_slice(&self.denominator.to_be_bytes());
        bytes
    }

    fn from_i128(numerator: i128, denominator: i128) -> Result<Self, TimeError> {
        if denominator == 0 {
            return Err(TimeError::ZeroDenominator);
        }
        if numerator == 0 {
            return Ok(Self {
                numerator: 0,
                denominator: 1,
            });
        }

        let greatest_common_divisor = gcd(numerator.unsigned_abs(), denominator.unsigned_abs());
        let divisor = i128::try_from(greatest_common_divisor).map_err(|_| TimeError::Overflow)?;
        let numerator = numerator / divisor;
        let denominator = denominator / divisor;
        let (numerator, denominator) = if denominator < 0 {
            (-numerator, -denominator)
        } else {
            (numerator, denominator)
        };

        Ok(Self {
            numerator: i64::try_from(numerator).map_err(|_| TimeError::Overflow)?,
            denominator: i64::try_from(denominator).map_err(|_| TimeError::Overflow)?,
        })
    }
}

impl Ord for Rational {
    fn cmp(&self, other: &Self) -> Ordering {
        (i128::from(self.numerator) * i128::from(other.denominator))
            .cmp(&(i128::from(other.numerator) * i128::from(self.denominator)))
    }
}

impl PartialOrd for Rational {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// A canonical half-open range, `[start, end)`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimeRange {
    start: Rational,
    end: Rational,
}

impl TimeRange {
    /// Constructs a nonempty half-open range.
    ///
    /// # Errors
    ///
    /// Returns [`TimeError::InvalidRange`] when `start >= end`.
    pub fn new(start: Rational, end: Rational) -> Result<Self, TimeError> {
        if start >= end {
            return Err(TimeError::InvalidRange);
        }
        Ok(Self { start, end })
    }

    #[must_use]
    pub const fn start(self) -> Rational {
        self.start
    }

    #[must_use]
    pub const fn end(self) -> Rational {
        self.end
    }

    #[must_use]
    pub fn contains(self, time: Rational) -> bool {
        self.start <= time && time < self.end
    }

    #[must_use]
    pub fn canonical_bytes(self) -> [u8; 32] {
        let mut bytes = [0; 32];
        bytes[..16].copy_from_slice(&self.start.canonical_bytes());
        bytes[16..].copy_from_slice(&self.end.canonical_bytes());
        bytes
    }
}

/// Explicit rounding policy for the only boundary where canonical time becomes
/// a platform or media tick count.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoundingPolicy {
    Floor,
    Ceil,
    NearestEven,
}

/// Absolute-timestamp adapter unit. All variants name their tick rate exactly.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdapterUnit {
    /// 100-nanosecond Windows media time.
    Hns,
    /// Ticks per second from a container or media stream timescale.
    Timescale(u64),
    /// Audio samples per second.
    Samples(u64),
}

impl AdapterUnit {
    fn ticks_per_second(self) -> Result<i64, TimeError> {
        let rate = match self {
            Self::Hns => 10_000_000,
            Self::Timescale(rate) | Self::Samples(rate) => rate,
        };
        if rate == 0 {
            return Err(TimeError::InvalidUnit);
        }
        i64::try_from(rate).map_err(|_| TimeError::Overflow)
    }
}

/// The trace emitted by an adapter: original canonical time, named policy,
/// selected tick count, and the exact conversion error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdaptedTime {
    pub canonical_time: Rational,
    pub unit: AdapterUnit,
    pub policy: RoundingPolicy,
    pub ticks: i64,
    pub error: Rational,
}

/// Adapts an absolute canonical time once. It never accumulates rounded duration
/// deltas, preventing drift across a long timeline.
///
/// # Errors
///
/// Returns [`TimeError`] if the adapter unit is invalid or the conversion does
/// not fit the canonical tick representation.
pub fn adapt_absolute(
    canonical_time: Rational,
    unit: AdapterUnit,
    policy: RoundingPolicy,
) -> Result<AdaptedTime, TimeError> {
    let rate = unit.ticks_per_second()?;
    let scaled_numerator = i128::from(canonical_time.numerator) * i128::from(rate);
    let denominator = i128::from(canonical_time.denominator);
    let ticks = round_quotient(scaled_numerator, denominator, policy)?;
    let ticks_i64 = i64::try_from(ticks).map_err(|_| TimeError::Overflow)?;
    let represented = Rational::new(ticks_i64, rate)?;
    Ok(AdaptedTime {
        canonical_time,
        unit,
        policy,
        ticks: ticks_i64,
        error: canonical_time.checked_sub(represented)?,
    })
}

/// Adapts a strictly increasing vector of absolute timestamps, preserving the
/// source order and rejecting duplicate or decreasing canonical positions.
///
/// # Errors
///
/// Returns [`TimeError::InvalidRange`] for non-increasing source timestamps and
/// [`TimeError::NonMonotonicAdaptedTicks`] when rounding would collapse output
/// ticks.
pub fn adapt_absolute_sequence(
    canonical_times: &[Rational],
    unit: AdapterUnit,
    policy: RoundingPolicy,
) -> Result<Vec<AdaptedTime>, TimeError> {
    let mut adapted = Vec::with_capacity(canonical_times.len());
    let mut previous = None;
    for time in canonical_times {
        if previous.is_some_and(|previous_time| *time <= previous_time) {
            return Err(TimeError::InvalidRange);
        }
        let next = adapt_absolute(*time, unit, policy)?;
        if adapted
            .last()
            .is_some_and(|previous_time: &AdaptedTime| next.ticks <= previous_time.ticks)
        {
            return Err(TimeError::NonMonotonicAdaptedTicks);
        }
        adapted.push(next);
        previous = Some(*time);
    }
    Ok(adapted)
}

fn round_quotient(
    numerator: i128,
    denominator: i128,
    policy: RoundingPolicy,
) -> Result<i128, TimeError> {
    debug_assert!(denominator > 0);
    let quotient = numerator.div_euclid(denominator);
    let remainder = numerator.rem_euclid(denominator);
    match policy {
        RoundingPolicy::Floor => Ok(quotient),
        RoundingPolicy::Ceil => Ok(quotient + i128::from(remainder != 0)),
        RoundingPolicy::NearestEven => {
            let doubled_remainder = remainder.checked_mul(2).ok_or(TimeError::Overflow)?;
            match doubled_remainder.cmp(&denominator) {
                Ordering::Less => Ok(quotient),
                Ordering::Equal if quotient.rem_euclid(2) == 0 => Ok(quotient),
                Ordering::Greater | Ordering::Equal => Ok(quotient + 1),
            }
        }
    }
}

fn gcd(mut left: u128, mut right: u128) -> u128 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

/// A stable 128-bit identifier. Its bytes, not display formatting, are part of
/// canonical project encoding.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AssetId(pub [u8; 16]);

/// A stable 128-bit track identifier.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TrackId(pub [u8; 16]);

/// A stable 128-bit clip identifier.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ClipId(pub [u8; 16]);

/// Caller-supplied idempotency key for a durable command request.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RequestId(pub [u8; 16]);

/// A SHA-256 digest over explicitly versioned canonical bytes.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RevisionHash(pub [u8; 32]);

/// A SHA-256 digest of complete canonical project state, separate from revision identity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProjectDigest(pub [u8; 32]);

impl RevisionHash {
    #[must_use]
    pub fn as_bytes(self) -> [u8; 32] {
        self.0
    }
}

/// Minimal normalized project model for the T-0007 core slice.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Project {
    schema_version: u32,
    assets: OrdMap<AssetId, Asset>,
    tracks: OrdMap<TrackId, Track>,
    clips: OrdMap<ClipId, Clip>,
}

/// A version-one, portable identity record for source media. It has no path,
/// handle, decoder, or OS object in canonical state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Asset {
    pub id: AssetId,
    pub identity: AssetIdentity,
}

/// Stable source identity used to detect different media behind a provider.
/// `provider_id` is the provider-stable namespace/locator identity, not a fresh
/// per-import random value. A caller must preserve it when reopening the same
/// provider object.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AssetIdentity {
    pub content_sha256: [u8; 32],
    pub byte_length: u64,
    pub provider_id: [u8; 16],
}

/// A track has a deterministic clip order, independent of command insertion
/// order elsewhere in the model.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Track {
    id: TrackId,
    clip_ids: Vector<ClipId>,
}

/// Immutable clip placement and source selection in canonical timeline time.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Clip {
    id: ClipId,
    asset_id: AssetId,
    track_id: TrackId,
    timeline_range: TimeRange,
    source_range: TimeRange,
}

/// Typed project edit commands. No UI, native, codec, or media object crosses
/// this stable contract boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Command {
    AddAsset {
        asset: Asset,
    },
    AddTrack {
        track_id: TrackId,
    },
    AddClip {
        clip_id: ClipId,
        asset_id: AssetId,
        track_id: TrackId,
        timeline_range: TimeRange,
        source_range: TimeRange,
    },
    MoveClip {
        clip_id: ClipId,
        timeline_range: TimeRange,
    },
    RemoveClip {
        clip_id: ClipId,
    },
}

/// A command plus its caller-supplied durable idempotency key.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandRequest {
    pub request_id: RequestId,
    pub command: Command,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ModelError {
    DuplicateAsset(AssetId),
    MissingAsset(AssetId),
    DuplicateTrack(TrackId),
    MissingTrack(TrackId),
    DuplicateClip(ClipId),
    MissingClip(ClipId),
    CorruptProject,
}

impl fmt::Display for ModelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateAsset(_) => formatter.write_str("asset already exists"),
            Self::MissingAsset(_) => formatter.write_str("asset does not exist"),
            Self::DuplicateTrack(_) => formatter.write_str("track already exists"),
            Self::MissingTrack(_) => formatter.write_str("track does not exist"),
            Self::DuplicateClip(_) => formatter.write_str("clip already exists"),
            Self::MissingClip(_) => formatter.write_str("clip does not exist"),
            Self::CorruptProject => formatter.write_str("project canonical bytes are invalid"),
        }
    }
}

impl std::error::Error for ModelError {}

impl Default for Project {
    fn default() -> Self {
        Self {
            schema_version: CONTRACT_SCHEMA_VERSION,
            assets: OrdMap::new(),
            tracks: OrdMap::new(),
            clips: OrdMap::new(),
        }
    }
}

impl Project {
    #[must_use]
    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    #[must_use]
    pub fn track(&self, track_id: TrackId) -> Option<&Track> {
        self.tracks.get(&track_id)
    }

    #[must_use]
    pub fn asset(&self, asset_id: AssetId) -> Option<&Asset> {
        self.assets.get(&asset_id)
    }

    #[must_use]
    pub fn clip(&self, clip_id: ClipId) -> Option<&Clip> {
        self.clips.get(&clip_id)
    }

    #[must_use]
    pub fn asset_count(&self) -> usize {
        self.assets.len()
    }

    #[must_use]
    pub fn track_count(&self) -> usize {
        self.tracks.len()
    }

    #[must_use]
    pub fn clip_count(&self) -> usize {
        self.clips.len()
    }

    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"STITCH-PROJECT\0");
        write_u32(&mut bytes, self.schema_version);
        write_u64(&mut bytes, self.assets.len());
        for asset in self.assets.values() {
            bytes.extend_from_slice(&asset.id.0);
            bytes.extend_from_slice(&asset.identity.content_sha256);
            bytes.extend_from_slice(&asset.identity.byte_length.to_be_bytes());
            bytes.extend_from_slice(&asset.identity.provider_id);
        }
        write_u64(&mut bytes, self.tracks.len());
        for track in self.tracks.values() {
            bytes.extend_from_slice(&track.id.0);
            write_u64(&mut bytes, track.clip_ids.len());
            for clip_id in &track.clip_ids {
                bytes.extend_from_slice(&clip_id.0);
            }
        }
        write_u64(&mut bytes, self.clips.len());
        for clip in self.clips.values() {
            bytes.extend_from_slice(&clip.id.0);
            bytes.extend_from_slice(&clip.asset_id.0);
            bytes.extend_from_slice(&clip.track_id.0);
            bytes.extend_from_slice(&clip.timeline_range.canonical_bytes());
            bytes.extend_from_slice(&clip.source_range.canonical_bytes());
        }
        bytes
    }

    /// Decodes only the exact version-one canonical representation.
    ///
    /// # Errors
    ///
    /// Returns [`ModelError::CorruptProject`] for an unknown schema, invalid
    /// lengths/tags, duplicate identities, inconsistent references, or trailing
    /// bytes. It never accepts a best-effort or partially decoded project.
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, ModelError> {
        let mut reader = Reader::new(bytes);
        reader.expect(b"STITCH-PROJECT\0")?;
        if reader.u32()? != CONTRACT_SCHEMA_VERSION {
            return Err(ModelError::CorruptProject);
        }
        let mut assets = OrdMap::new();
        for _ in 0..reader.count()? {
            let id = AssetId(reader.array()?);
            let identity = AssetIdentity {
                content_sha256: reader.array()?,
                byte_length: reader.u64()?,
                provider_id: reader.array()?,
            };
            if assets.insert(id, Asset::new(id, identity)).is_some() {
                return Err(ModelError::CorruptProject);
            }
        }

        let mut tracks = OrdMap::new();
        for _ in 0..reader.count()? {
            let id = TrackId(reader.array()?);
            let clip_count = reader.count()?;
            let mut clip_ids = Vector::new();
            for _ in 0..clip_count {
                clip_ids.push_back(ClipId(reader.array()?));
            }
            if tracks.insert(id, Track { id, clip_ids }).is_some() {
                return Err(ModelError::CorruptProject);
            }
        }

        let mut clips = OrdMap::new();
        for _ in 0..reader.count()? {
            let id = ClipId(reader.array()?);
            let clip = Clip {
                id,
                asset_id: AssetId(reader.array()?),
                track_id: TrackId(reader.array()?),
                timeline_range: reader.time_range()?,
                source_range: reader.time_range()?,
            };
            if clips.insert(id, clip).is_some() {
                return Err(ModelError::CorruptProject);
            }
        }
        reader.finish()?;

        let project = Self {
            schema_version: CONTRACT_SCHEMA_VERSION,
            assets,
            tracks,
            clips,
        };
        project.validate()?;
        Ok(project)
    }

    #[must_use]
    pub fn semantic_digest(&self) -> ProjectDigest {
        ProjectDigest(Sha256::digest(self.canonical_bytes()).into())
    }

    fn validate(&self) -> Result<(), ModelError> {
        let mut listed_clips = BTreeSet::new();
        for (track_id, track) in &self.tracks {
            if track.id != *track_id {
                return Err(ModelError::CorruptProject);
            }
            for clip_id in &track.clip_ids {
                if !listed_clips.insert(*clip_id) {
                    return Err(ModelError::CorruptProject);
                }
                let clip = self.clips.get(clip_id).ok_or(ModelError::CorruptProject)?;
                if clip.track_id != *track_id {
                    return Err(ModelError::CorruptProject);
                }
            }
        }
        if listed_clips.len() != self.clips.len() {
            return Err(ModelError::CorruptProject);
        }
        for clip in self.clips.values() {
            if !self.assets.contains_key(&clip.asset_id)
                || !self.tracks.contains_key(&clip.track_id)
            {
                return Err(ModelError::CorruptProject);
            }
        }
        Ok(())
    }
}

impl Asset {
    #[must_use]
    pub const fn new(id: AssetId, identity: AssetIdentity) -> Self {
        Self { id, identity }
    }
}

impl Track {
    #[must_use]
    pub const fn id(&self) -> TrackId {
        self.id
    }

    #[must_use]
    pub fn clip_ids(&self) -> &Vector<ClipId> {
        &self.clip_ids
    }
}

impl Clip {
    #[must_use]
    pub const fn id(&self) -> ClipId {
        self.id
    }

    #[must_use]
    pub const fn asset_id(&self) -> AssetId {
        self.asset_id
    }

    #[must_use]
    pub const fn track_id(&self) -> TrackId {
        self.track_id
    }

    #[must_use]
    pub const fn timeline_range(&self) -> TimeRange {
        self.timeline_range
    }
}

impl Command {
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"STITCH-COMMAND\0");
        write_u32(&mut bytes, CONTRACT_SCHEMA_VERSION);
        match self {
            Self::AddAsset { asset } => {
                bytes.push(1);
                bytes.extend_from_slice(&asset.id.0);
                bytes.extend_from_slice(&asset.identity.content_sha256);
                bytes.extend_from_slice(&asset.identity.byte_length.to_be_bytes());
                bytes.extend_from_slice(&asset.identity.provider_id);
            }
            Self::AddTrack { track_id } => {
                bytes.push(2);
                bytes.extend_from_slice(&track_id.0);
            }
            Self::AddClip {
                clip_id,
                asset_id,
                track_id,
                timeline_range,
                source_range,
            } => {
                bytes.push(3);
                bytes.extend_from_slice(&clip_id.0);
                bytes.extend_from_slice(&asset_id.0);
                bytes.extend_from_slice(&track_id.0);
                bytes.extend_from_slice(&timeline_range.canonical_bytes());
                bytes.extend_from_slice(&source_range.canonical_bytes());
            }
            Self::MoveClip {
                clip_id,
                timeline_range,
            } => {
                bytes.push(4);
                bytes.extend_from_slice(&clip_id.0);
                bytes.extend_from_slice(&timeline_range.canonical_bytes());
            }
            Self::RemoveClip { clip_id } => {
                bytes.push(5);
                bytes.extend_from_slice(&clip_id.0);
            }
        }
        bytes
    }

    /// Decodes only the exact version-one canonical command representation.
    ///
    /// # Errors
    ///
    /// Returns [`ModelError::CorruptProject`] for unknown tags, invalid time
    /// values, truncated input, or trailing bytes.
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, ModelError> {
        let mut reader = Reader::new(bytes);
        reader.expect(b"STITCH-COMMAND\0")?;
        if reader.u32()? != CONTRACT_SCHEMA_VERSION {
            return Err(ModelError::CorruptProject);
        }
        let command = match reader.byte()? {
            1 => Self::AddAsset {
                asset: Asset::new(
                    AssetId(reader.array()?),
                    AssetIdentity {
                        content_sha256: reader.array()?,
                        byte_length: reader.u64()?,
                        provider_id: reader.array()?,
                    },
                ),
            },
            2 => Self::AddTrack {
                track_id: TrackId(reader.array()?),
            },
            3 => Self::AddClip {
                clip_id: ClipId(reader.array()?),
                asset_id: AssetId(reader.array()?),
                track_id: TrackId(reader.array()?),
                timeline_range: reader.time_range()?,
                source_range: reader.time_range()?,
            },
            4 => Self::MoveClip {
                clip_id: ClipId(reader.array()?),
                timeline_range: reader.time_range()?,
            },
            5 => Self::RemoveClip {
                clip_id: ClipId(reader.array()?),
            },
            _ => return Err(ModelError::CorruptProject),
        };
        reader.finish()?;
        Ok(command)
    }
}

/// Applies a typed command atomically: on error, `project` is untouched.
///
/// # Errors
///
/// Returns [`ModelError`] when the command preconditions do not hold.
pub fn reduce(project: &Project, command: &Command) -> Result<Project, ModelError> {
    let mut next = project.clone();
    match command {
        Command::AddAsset { asset } => {
            if next.assets.contains_key(&asset.id) {
                return Err(ModelError::DuplicateAsset(asset.id));
            }
            next.assets.insert(asset.id, asset.clone());
        }
        Command::AddTrack { track_id } => {
            if next.tracks.contains_key(track_id) {
                return Err(ModelError::DuplicateTrack(*track_id));
            }
            next.tracks.insert(
                *track_id,
                Track {
                    id: *track_id,
                    clip_ids: Vector::new(),
                },
            );
        }
        Command::AddClip {
            clip_id,
            asset_id,
            track_id,
            timeline_range,
            source_range,
        } => {
            if next.clips.contains_key(clip_id) {
                return Err(ModelError::DuplicateClip(*clip_id));
            }
            if !next.assets.contains_key(asset_id) {
                return Err(ModelError::MissingAsset(*asset_id));
            }
            let mut track = next
                .tracks
                .get(track_id)
                .cloned()
                .ok_or(ModelError::MissingTrack(*track_id))?;
            track.clip_ids.push_back(*clip_id);
            next.tracks.insert(*track_id, track);
            next.clips.insert(
                *clip_id,
                Clip {
                    id: *clip_id,
                    asset_id: *asset_id,
                    track_id: *track_id,
                    timeline_range: *timeline_range,
                    source_range: *source_range,
                },
            );
        }
        Command::MoveClip {
            clip_id,
            timeline_range,
        } => {
            let mut clip = next
                .clips
                .get(clip_id)
                .cloned()
                .ok_or(ModelError::MissingClip(*clip_id))?;
            clip.timeline_range = *timeline_range;
            next.clips.insert(*clip_id, clip);
        }
        Command::RemoveClip { clip_id } => {
            let clip = next
                .clips
                .get(clip_id)
                .cloned()
                .ok_or(ModelError::MissingClip(*clip_id))?;
            let mut track = next
                .tracks
                .get(&clip.track_id)
                .cloned()
                .ok_or(ModelError::CorruptProject)?;
            track.clip_ids = track
                .clip_ids
                .iter()
                .copied()
                .filter(|entry| entry != clip_id)
                .collect();
            next.tracks.insert(clip.track_id, track);
            next.clips.remove(clip_id);
        }
    }
    Ok(next)
}

/// Computes the domain-separated immutable command-chain hash.
///
/// The only legal parent is the active head produced by replay from genesis;
/// this function intentionally cannot accept a caller-supplied project.
#[must_use]
pub fn revision_hash(parent: Option<RevisionHash>, command: &Command) -> RevisionHash {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"STITCH-REVISION\0");
    bytes.extend_from_slice(&CONTRACT_SCHEMA_VERSION.to_be_bytes());
    match parent {
        Some(parent) => {
            bytes.push(1);
            bytes.extend_from_slice(&parent.0);
        }
        None => bytes.push(0),
    }
    write_bytes(&mut bytes, &command.canonical_bytes());
    hash_bytes(&bytes)
}

fn hash_bytes(bytes: &[u8]) -> RevisionHash {
    RevisionHash(Sha256::digest(bytes).into())
}

fn write_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

fn write_u64(bytes: &mut Vec<u8>, value: usize) {
    bytes.extend_from_slice(&(value as u64).to_be_bytes());
}

fn write_bytes(target: &mut Vec<u8>, value: &[u8]) {
    write_u64(target, value.len());
    target.extend_from_slice(value);
}

struct Reader<'a> {
    input: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    const fn new(input: &'a [u8]) -> Self {
        Self { input, offset: 0 }
    }

    fn expect(&mut self, expected: &[u8]) -> Result<(), ModelError> {
        if self.take(expected.len())? == expected {
            Ok(())
        } else {
            Err(ModelError::CorruptProject)
        }
    }

    fn array<const LENGTH: usize>(&mut self) -> Result<[u8; LENGTH], ModelError> {
        self.take(LENGTH)?
            .try_into()
            .map_err(|_| ModelError::CorruptProject)
    }

    fn u32(&mut self) -> Result<u32, ModelError> {
        Ok(u32::from_be_bytes(self.array()?))
    }

    fn u64(&mut self) -> Result<u64, ModelError> {
        Ok(u64::from_be_bytes(self.array()?))
    }

    fn byte(&mut self) -> Result<u8, ModelError> {
        Ok(self.array::<1>()?[0])
    }

    fn count(&mut self) -> Result<usize, ModelError> {
        usize::try_from(self.u64()?).map_err(|_| ModelError::CorruptProject)
    }

    fn rational(&mut self) -> Result<Rational, ModelError> {
        Rational::new(
            i64::from_be_bytes(self.array()?),
            i64::from_be_bytes(self.array()?),
        )
        .map_err(|_| ModelError::CorruptProject)
    }

    fn time_range(&mut self) -> Result<TimeRange, ModelError> {
        TimeRange::new(self.rational()?, self.rational()?).map_err(|_| ModelError::CorruptProject)
    }

    fn finish(self) -> Result<(), ModelError> {
        if self.offset == self.input.len() {
            Ok(())
        } else {
            Err(ModelError::CorruptProject)
        }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], ModelError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(ModelError::CorruptProject)?;
        let value = self
            .input
            .get(self.offset..end)
            .ok_or(ModelError::CorruptProject)?;
        self.offset = end;
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AdapterUnit, Asset, AssetId, AssetIdentity, ClipId, Command, ModelError, Project,
        ProjectDigest, Rational, RevisionHash, RoundingPolicy, TimeError, TimeRange, TrackId,
        adapt_absolute, adapt_absolute_sequence, reduce, revision_hash,
    };

    #[test]
    fn rational_is_normalized_and_has_one_canonical_zero() {
        assert_eq!(Rational::new(-2, -4), Ok(Rational::new(1, 2).unwrap()));
        assert_eq!(Rational::new(0, -99), Ok(Rational::new(0, 1).unwrap()));
        assert_eq!(Rational::new(1, 0), Err(TimeError::ZeroDenominator));
    }

    #[test]
    fn checked_math_reduces_before_narrowing_an_intermediate() {
        let maximum = Rational::new(i64::MAX, i64::MAX - 1).unwrap();
        let inverse = Rational::new(i64::MAX - 1, i64::MAX).unwrap();
        assert_eq!(
            maximum.checked_mul(inverse),
            Ok(Rational::new(1, 1).unwrap())
        );
    }

    #[test]
    fn rational_seeded_properties_and_overflow_boundaries_are_checked() {
        let mut state = 0x9e37_79b9_7f4a_7c15_u64;
        for _ in 0..10_000 {
            state = xorshift(state);
            let numerator = state.cast_signed();
            state = xorshift(state);
            let denominator = i64::try_from((state >> 1) + 1).unwrap();
            let value = Rational::new(numerator, denominator).unwrap();
            assert!(value.denominator() > 0);
            assert_eq!(
                unsigned_gcd(
                    value.numerator().unsigned_abs(),
                    value.denominator().unsigned_abs(),
                ),
                1
            );

            state = xorshift(state);
            let other = Rational::new(state.cast_signed(), denominator).unwrap();
            assert_eq!(
                value.cmp(&other),
                (i128::from(value.numerator()) * i128::from(other.denominator()))
                    .cmp(&(i128::from(other.numerator()) * i128::from(value.denominator())),)
            );
            if let Ok(sum) = value.checked_add(other) {
                assert_eq!(sum.checked_sub(other), Ok(value));
            }
        }

        let maximum = Rational::new(i64::MAX, 1).unwrap();
        let minimum = Rational::new(i64::MIN, 1).unwrap();
        assert_eq!(
            maximum.checked_add(Rational::new(1, 1).unwrap()),
            Err(TimeError::Overflow)
        );
        assert_eq!(
            minimum.checked_sub(Rational::new(1, 1).unwrap()),
            Err(TimeError::Overflow)
        );
        assert_eq!(
            maximum.checked_mul(Rational::new(2, 1).unwrap()),
            Err(TimeError::Overflow)
        );
        assert_eq!(Rational::new(i64::MIN, -1), Err(TimeError::Overflow));
    }

    #[test]
    fn ranges_are_half_open() {
        let range =
            TimeRange::new(Rational::new(1, 1).unwrap(), Rational::new(2, 1).unwrap()).unwrap();
        assert!(range.contains(Rational::new(1, 1).unwrap()));
        assert!(!range.contains(Rational::new(2, 1).unwrap()));
        assert_eq!(
            TimeRange::new(range.end(), range.start()),
            Err(TimeError::InvalidRange)
        );
    }

    #[test]
    fn adapter_names_and_reports_nearest_even_error() {
        let adapted = adapt_absolute(
            Rational::new(3, 20_000_000).unwrap(),
            AdapterUnit::Hns,
            RoundingPolicy::NearestEven,
        )
        .unwrap();
        assert_eq!(adapted.ticks, 2);
        assert_eq!(adapted.error, Rational::new(-1, 20_000_000).unwrap());
    }

    #[test]
    fn adapter_rounding_has_explicit_negative_boundaries() {
        let time = Rational::new(-3, 20_000_000).unwrap();
        assert_eq!(
            adapt_absolute(time, AdapterUnit::Hns, RoundingPolicy::Floor)
                .unwrap()
                .ticks,
            -2
        );
        assert_eq!(
            adapt_absolute(time, AdapterUnit::Hns, RoundingPolicy::Ceil)
                .unwrap()
                .ticks,
            -1
        );
        assert_eq!(
            adapt_absolute(time, AdapterUnit::Hns, RoundingPolicy::NearestEven)
                .unwrap()
                .ticks,
            -2
        );
    }

    #[test]
    fn adapter_rejects_collapsed_ticks() {
        let source = [Rational::new(0, 1).unwrap(), Rational::new(1, 100).unwrap()];
        assert_eq!(
            adapt_absolute_sequence(
                &source,
                AdapterUnit::Timescale(1),
                RoundingPolicy::NearestEven,
            ),
            Err(TimeError::NonMonotonicAdaptedTicks)
        );
    }

    #[test]
    fn ten_thousand_cfr_vectors_have_bounded_absolute_adapter_error() {
        for frame_rate in [
            Rational::new(24_000, 1_001).unwrap(),
            Rational::new(30_000, 1_001).unwrap(),
            Rational::new(60_000, 1_001).unwrap(),
        ] {
            let frame_duration = frame_rate.checked_reciprocal().unwrap();
            let timestamps = (0..10_000)
                .map(|index| {
                    frame_duration
                        .checked_mul(Rational::new(index, 1).unwrap())
                        .unwrap()
                })
                .collect::<Vec<_>>();
            let adapted =
                adapt_absolute_sequence(&timestamps, AdapterUnit::Hns, RoundingPolicy::NearestEven)
                    .unwrap();

            assert_eq!(adapted.len(), 10_000);
            for pair in adapted.windows(2) {
                assert!(pair[0].ticks < pair[1].ticks);
            }
            for entry in adapted {
                assert!(entry.error <= Rational::new(1, 20_000_000).unwrap());
                assert!(entry.error >= Rational::new(-1, 20_000_000).unwrap());
            }
        }
    }

    #[test]
    fn vfr_vector_keeps_each_absolute_source_timestamp() {
        let source = [
            Rational::new(0, 1).unwrap(),
            Rational::new(1, 24).unwrap(),
            Rational::new(7, 100).unwrap(),
            Rational::new(1, 5).unwrap(),
        ];
        let adapted = adapt_absolute_sequence(
            &source,
            AdapterUnit::Timescale(90_000),
            RoundingPolicy::NearestEven,
        )
        .unwrap();
        assert_eq!(adapted[2].canonical_time, Rational::new(7, 100).unwrap());
        assert_eq!(adapted[2].ticks, 6_300);
    }

    #[test]
    fn canonical_project_bytes_ignore_independent_command_order() {
        let asset_id = AssetId([1; 16]);
        let track_id = TrackId([2; 16]);
        let clip_id = ClipId([3; 16]);
        let range =
            TimeRange::new(Rational::new(0, 1).unwrap(), Rational::new(1, 1).unwrap()).unwrap();
        let clip = Command::AddClip {
            clip_id,
            asset_id,
            track_id,
            timeline_range: range,
            source_range: range,
        };
        let first = reduce(
            &reduce(
                &reduce(
                    &Project::default(),
                    &Command::AddAsset {
                        asset: asset(asset_id),
                    },
                )
                .unwrap(),
                &Command::AddTrack { track_id },
            )
            .unwrap(),
            &clip,
        )
        .unwrap();
        let second = reduce(
            &reduce(
                &reduce(&Project::default(), &Command::AddTrack { track_id }).unwrap(),
                &Command::AddAsset {
                    asset: asset(asset_id),
                },
            )
            .unwrap(),
            &clip,
        )
        .unwrap();
        assert_eq!(first.canonical_bytes(), second.canonical_bytes());
        assert_eq!(first.semantic_digest(), second.semantic_digest());
    }

    #[test]
    fn invalid_command_has_no_partial_mutation() {
        let project = reduce(
            &Project::default(),
            &Command::AddTrack {
                track_id: TrackId([9; 16]),
            },
        )
        .unwrap();
        let before = project.canonical_bytes();
        let error = reduce(
            &project,
            &Command::AddClip {
                clip_id: ClipId([8; 16]),
                asset_id: AssetId([7; 16]),
                track_id: TrackId([9; 16]),
                timeline_range: TimeRange::new(
                    Rational::new(0, 1).unwrap(),
                    Rational::new(1, 1).unwrap(),
                )
                .unwrap(),
                source_range: TimeRange::new(
                    Rational::new(0, 1).unwrap(),
                    Rational::new(1, 1).unwrap(),
                )
                .unwrap(),
            },
        );
        assert_eq!(error, Err(ModelError::MissingAsset(AssetId([7; 16]))));
        assert_eq!(project.canonical_bytes(), before);
    }

    #[test]
    fn revision_hash_is_deterministic_and_parent_sensitive() {
        let command = Command::AddAsset {
            asset: asset(AssetId([4; 16])),
        };
        let first = revision_hash(None, &command);
        let repeated = revision_hash(None, &command);
        let with_parent = revision_hash(Some(first), &command);
        assert_eq!(first, repeated);
        assert_ne!(first, with_parent);
    }

    #[test]
    fn canonical_bytes_and_sha256_digests_match_fixed_goldens() {
        let command = Command::AddTrack {
            track_id: TrackId([0x2a; 16]),
        };
        assert_eq!(
            command.canonical_bytes(),
            vec![
                0x53, 0x54, 0x49, 0x54, 0x43, 0x48, 0x2d, 0x43, 0x4f, 0x4d, 0x4d, 0x41, 0x4e, 0x44,
                0x00, 0x00, 0x00, 0x00, 0x01, 0x02, 0x2a, 0x2a, 0x2a, 0x2a, 0x2a, 0x2a, 0x2a, 0x2a,
                0x2a, 0x2a, 0x2a, 0x2a, 0x2a, 0x2a, 0x2a, 0x2a,
            ]
        );
        assert_eq!(
            Project::default().canonical_bytes(),
            vec![
                0x53, 0x54, 0x49, 0x54, 0x43, 0x48, 0x2d, 0x50, 0x52, 0x4f, 0x4a, 0x45, 0x43, 0x54,
                0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00,
            ]
        );
        assert_eq!(
            Project::default().semantic_digest(),
            ProjectDigest([
                0x36, 0xbb, 0x93, 0x8b, 0x6c, 0x56, 0xa2, 0xa9, 0xa2, 0x3b, 0x62, 0x05, 0x7e, 0xde,
                0xd2, 0x5c, 0x41, 0xef, 0x1f, 0x30, 0xa1, 0xd2, 0xaf, 0xd8, 0xe1, 0x04, 0xbb, 0x56,
                0x99, 0xd0, 0xc3, 0xc2,
            ])
        );
        assert_eq!(
            revision_hash(None, &command),
            RevisionHash([
                0xb0, 0x67, 0x50, 0xd0, 0xe6, 0x2e, 0xce, 0x07, 0x3c, 0xf0, 0x06, 0xce, 0xea, 0x93,
                0x80, 0x31, 0x8a, 0xf4, 0x24, 0x65, 0xc0, 0x46, 0xe0, 0x4a, 0xd8, 0xf0, 0xf5, 0x56,
                0x53, 0xef, 0xbe, 0x26,
            ])
        );
    }

    #[test]
    fn command_encoding_is_versioned_round_trips_and_rejects_adversarial_bytes() {
        let command = Command::AddAsset {
            asset: asset(AssetId([11; 16])),
        };
        let bytes = command.canonical_bytes();
        assert_eq!(Command::from_canonical_bytes(&bytes), Ok(command));
        assert_eq!(
            Command::from_canonical_bytes(&bytes[..bytes.len() - 1]),
            Err(ModelError::CorruptProject)
        );
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert_eq!(
            Command::from_canonical_bytes(&trailing),
            Err(ModelError::CorruptProject)
        );
        let mut unknown_version = bytes.clone();
        unknown_version[18] = 2;
        assert_eq!(
            Command::from_canonical_bytes(&unknown_version),
            Err(ModelError::CorruptProject)
        );
        let mut unknown_tag = bytes;
        unknown_tag[19] = 99;
        assert_eq!(
            Command::from_canonical_bytes(&unknown_tag),
            Err(ModelError::CorruptProject)
        );
    }

    #[test]
    fn every_command_variant_round_trips_and_every_prefix_is_rejected() {
        let range =
            TimeRange::new(Rational::new(0, 1).unwrap(), Rational::new(1, 1).unwrap()).unwrap();
        let commands = [
            Command::AddAsset {
                asset: asset(AssetId([1; 16])),
            },
            Command::AddTrack {
                track_id: TrackId([2; 16]),
            },
            Command::AddClip {
                clip_id: ClipId([3; 16]),
                asset_id: AssetId([1; 16]),
                track_id: TrackId([2; 16]),
                timeline_range: range,
                source_range: range,
            },
            Command::MoveClip {
                clip_id: ClipId([3; 16]),
                timeline_range: range,
            },
            Command::RemoveClip {
                clip_id: ClipId([3; 16]),
            },
        ];
        for command in commands {
            let bytes = command.canonical_bytes();
            assert_eq!(Command::from_canonical_bytes(&bytes), Ok(command));
            for length in 0..bytes.len() {
                assert_eq!(
                    Command::from_canonical_bytes(&bytes[..length]),
                    Err(ModelError::CorruptProject)
                );
            }
        }
    }

    #[test]
    fn project_decode_rejects_trailing_and_corrupt_schema() {
        let bytes = Project::default().canonical_bytes();
        assert_eq!(
            Project::from_canonical_bytes(&bytes),
            Ok(Project::default())
        );
        for length in 0..bytes.len() {
            assert_eq!(
                Project::from_canonical_bytes(&bytes[..length]),
                Err(ModelError::CorruptProject)
            );
        }
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert_eq!(
            Project::from_canonical_bytes(&trailing),
            Err(ModelError::CorruptProject)
        );
        let mut version = bytes;
        version[18] = 2;
        assert_eq!(
            Project::from_canonical_bytes(&version),
            Err(ModelError::CorruptProject)
        );
    }

    #[test]
    fn project_decode_rejects_counts_duplicates_and_broken_references() {
        let mut absurd_count = Project::default().canonical_bytes();
        absurd_count[19..27].copy_from_slice(&u64::MAX.to_be_bytes());
        assert_eq!(
            Project::from_canonical_bytes(&absurd_count),
            Err(ModelError::CorruptProject)
        );

        let two_assets = reduce(
            &reduce(
                &Project::default(),
                &Command::AddAsset {
                    asset: asset(AssetId([1; 16])),
                },
            )
            .unwrap(),
            &Command::AddAsset {
                asset: asset(AssetId([2; 16])),
            },
        )
        .unwrap();
        let mut duplicate_asset = two_assets.canonical_bytes();
        let first_asset_id: [u8; 16] = duplicate_asset[27..43].try_into().unwrap();
        duplicate_asset[99..115].copy_from_slice(&first_asset_id);
        assert_eq!(
            Project::from_canonical_bytes(&duplicate_asset),
            Err(ModelError::CorruptProject)
        );

        let range =
            TimeRange::new(Rational::new(0, 1).unwrap(), Rational::new(1, 1).unwrap()).unwrap();
        let with_clip = reduce(
            &reduce(
                &reduce(
                    &Project::default(),
                    &Command::AddAsset {
                        asset: asset(AssetId([1; 16])),
                    },
                )
                .unwrap(),
                &Command::AddTrack {
                    track_id: TrackId([2; 16]),
                },
            )
            .unwrap(),
            &Command::AddClip {
                clip_id: ClipId([3; 16]),
                asset_id: AssetId([1; 16]),
                track_id: TrackId([2; 16]),
                timeline_range: range,
                source_range: range,
            },
        )
        .unwrap();
        let mut missing_asset = with_clip.canonical_bytes();
        missing_asset[171..187].fill(0xff);
        assert_eq!(
            Project::from_canonical_bytes(&missing_asset),
            Err(ModelError::CorruptProject)
        );

        let with_two_clips = reduce(
            &with_clip,
            &Command::AddClip {
                clip_id: ClipId([4; 16]),
                asset_id: AssetId([1; 16]),
                track_id: TrackId([2; 16]),
                timeline_range: range,
                source_range: range,
            },
        )
        .unwrap();
        let mut duplicate_track_entry = with_two_clips.canonical_bytes();
        let first_clip_id: [u8; 16] = duplicate_track_entry[131..147].try_into().unwrap();
        duplicate_track_entry[147..163].copy_from_slice(&first_clip_id);
        assert_eq!(
            Project::from_canonical_bytes(&duplicate_track_entry),
            Err(ModelError::CorruptProject)
        );
    }

    #[test]
    fn asset_identity_is_stable_and_content_or_size_changes_are_semantic() {
        let id = AssetId([7; 16]);
        let original =
            reduce(&Project::default(), &Command::AddAsset { asset: asset(id) }).unwrap();
        let reopened = Project::from_canonical_bytes(&original.canonical_bytes()).unwrap();
        assert_eq!(reopened.semantic_digest(), original.semantic_digest());

        let changed_content = reduce(
            &Project::default(),
            &Command::AddAsset {
                asset: Asset::new(
                    id,
                    AssetIdentity {
                        content_sha256: [8; 32],
                        byte_length: 42,
                        provider_id: [6; 16],
                    },
                ),
            },
        )
        .unwrap();
        let changed_size = reduce(
            &Project::default(),
            &Command::AddAsset {
                asset: Asset::new(
                    id,
                    AssetIdentity {
                        content_sha256: [5; 32],
                        byte_length: 43,
                        provider_id: [6; 16],
                    },
                ),
            },
        )
        .unwrap();
        assert_ne!(
            original.semantic_digest(),
            changed_content.semantic_digest()
        );
        assert_ne!(original.semantic_digest(), changed_size.semantic_digest());
    }

    fn xorshift(mut value: u64) -> u64 {
        value ^= value << 7;
        value ^= value >> 9;
        value ^ (value << 8)
    }

    fn unsigned_gcd(mut left: u64, mut right: u64) -> u64 {
        while right != 0 {
            let remainder = left % right;
            left = right;
            right = remainder;
        }
        left
    }

    fn asset(id: AssetId) -> Asset {
        Asset::new(
            id,
            AssetIdentity {
                content_sha256: [5; 32],
                byte_length: 42,
                provider_id: [6; 16],
            },
        )
    }
}
