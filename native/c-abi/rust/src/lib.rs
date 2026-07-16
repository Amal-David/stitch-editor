//! Small, versioned Rust-to-C control ABI. Native objects remain bridge-owned IDs.

#![allow(unsafe_code)]
#![allow(clippy::missing_safety_doc)]

use std::cell::Cell;
use std::collections::HashMap;
use std::mem::offset_of;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use stitch_editor_contracts::{
    Asset, AssetId, AssetIdentity, ClipId, Command, Rational, RevisionHash, TimeRange, TrackId,
};
use stitch_editor_core::EditorHistory;

const ABI_VERSION: u32 = 1;
const MAX_COMMANDS: usize = 4096;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct StitchHeader {
    pub struct_size: u32,
    pub version: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Default, Eq, PartialEq)]
pub struct StitchDigest {
    pub bytes: [u8; 32],
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct StitchId {
    pub bytes: [u8; 16],
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct StitchRational {
    pub numerator: i64,
    pub denominator: i64,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct StitchTimeRange {
    pub start: StitchRational,
    pub end: StitchRational,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct StitchResultDetail {
    pub header: StitchHeader,
    pub result: StitchResult,
    pub domain: u32,
    pub diagnostic: u64,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct StitchAbiNegotiation {
    pub header: StitchHeader,
    pub requested_major: u32,
    pub requested_minor: u32,
    pub supported_major: u32,
    pub supported_minor: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct StitchCoreConfig {
    pub header: StitchHeader,
    pub initial_epoch: u64,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct StitchCommand {
    pub header: StitchHeader,
    pub kind: u32,
    pub reserved: u32,
    pub primary_id: StitchId,
    pub secondary_id: StitchId,
    pub tertiary_id: StitchId,
    pub content_sha256: StitchDigest,
    pub byte_length: u64,
    pub provider_id: StitchId,
    pub timeline_range: StitchTimeRange,
    pub source_range: StitchTimeRange,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct StitchCommandBatch {
    pub header: StitchHeader,
    pub commands: *const StitchCommand,
    pub command_count: u32,
    pub reserved: u32,
    pub expected_revision: StitchDigest,
    pub epoch: u64,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct StitchSnapshotMetadata {
    pub header: StitchHeader,
    pub revision: StitchDigest,
    pub project_digest: StitchDigest,
    pub epoch: u64,
    pub asset_count: u64,
    pub track_count: u64,
    pub clip_count: u64,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct StitchDiffMetadata {
    pub header: StitchHeader,
    pub previous_revision: StitchDigest,
    pub revision: StitchDigest,
    pub project_digest: StitchDigest,
    pub epoch: u64,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct StitchFrameMetadata {
    pub header: StitchHeader,
    pub lease_id: u64,
    pub revision: StitchDigest,
    pub epoch: u64,
    pub device_generation: u64,
    pub surface_token: u64,
    pub synchronization_token: u64,
    pub owner_thread_token: u64,
    pub pixel_width: u32,
    pub pixel_height: u32,
    pub pixel_format: u32,
    pub synchronization_kind: u32,
    pub synchronization_value: u64,
    pub presentation_time: StitchRational,
    pub state: u32,
    pub reserved: u32,
}

const _: () = {
    assert!(size_of::<StitchHeader>() == 8);
    assert!(align_of::<StitchHeader>() == 4);
    assert!(size_of::<StitchResultDetail>() == 24);
    assert!(size_of::<StitchAbiNegotiation>() == 24);
    assert!(size_of::<StitchCoreConfig>() == 16);
    assert!(size_of::<StitchCommand>() == 184);
    assert!(align_of::<StitchCommand>() == 8);
    assert!(size_of::<StitchCommandBatch>() == 64);
    assert!(offset_of!(StitchCommandBatch, commands) == 8);
    assert!(offset_of!(StitchCommandBatch, expected_revision) == 24);
    assert!(size_of::<StitchSnapshotMetadata>() == 104);
    assert!(size_of::<StitchDiffMetadata>() == 112);
    assert!(size_of::<StitchFrameMetadata>() == 136);
    assert!(align_of::<StitchFrameMetadata>() == 8);
    assert!(offset_of!(StitchFrameMetadata, revision) == 16);
    assert!(offset_of!(StitchFrameMetadata, surface_token) == 64);
    assert!(offset_of!(StitchFrameMetadata, pixel_width) == 88);
    assert!(offset_of!(StitchFrameMetadata, synchronization_value) == 104);
    assert!(offset_of!(StitchFrameMetadata, presentation_time) == 112);
    assert!(offset_of!(StitchFrameMetadata, state) == 128);
};

#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StitchResult {
    Ok = 0,
    InvalidArgument = 1,
    UnsupportedVersion = 2,
    UnknownHandle = 3,
    MalformedStruct = 4,
    BadEnum = 5,
    BadRational = 6,
    InvalidBatch = 7,
    StaleRevision = 8,
    StaleEpoch = 9,
    StaleDevice = 10,
    BadLeaseState = 11,
    ReentrantMutation = 12,
    ModelError = 13,
    Panic = 14,
}

pub type StitchDiffCallback = Option<extern "C" fn(*mut core::ffi::c_void, u64)>;

#[derive(Clone, Copy, Eq, PartialEq)]
enum LeaseState {
    Registered = 1,
    Acquired = 2,
    Submitted = 3,
    Completed = 4,
    Retired = 5,
}
struct Lease {
    metadata: StitchFrameMetadata,
    state: LeaseState,
    owner: std::thread::ThreadId,
}
struct CoreState {
    history: EditorHistory,
    epoch: u64,
    leases: HashMap<u64, Lease>,
    owner: std::thread::ThreadId,
}
struct RefHandle<T> {
    value: T,
    refs: u32,
}

static NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);
static CORES: OnceLock<Mutex<HashMap<u64, Arc<Mutex<CoreState>>>>> = OnceLock::new();
static SNAPSHOTS: OnceLock<Mutex<HashMap<u64, RefHandle<StitchSnapshotMetadata>>>> =
    OnceLock::new();
static DIFFS: OnceLock<Mutex<HashMap<u64, RefHandle<StitchDiffMetadata>>>> = OnceLock::new();
thread_local! { static IN_CALLBACK: Cell<bool> = const { Cell::new(false) }; }

fn cores() -> &'static Mutex<HashMap<u64, Arc<Mutex<CoreState>>>> {
    CORES.get_or_init(|| Mutex::new(HashMap::new()))
}
fn snapshots() -> &'static Mutex<HashMap<u64, RefHandle<StitchSnapshotMetadata>>> {
    SNAPSHOTS.get_or_init(|| Mutex::new(HashMap::new()))
}
fn diffs() -> &'static Mutex<HashMap<u64, RefHandle<StitchDiffMetadata>>> {
    DIFFS.get_or_init(|| Mutex::new(HashMap::new()))
}
fn next_handle() -> u64 {
    NEXT_HANDLE.fetch_add(1, Ordering::Relaxed).max(1)
}
fn header<T>() -> StitchHeader {
    StitchHeader {
        struct_size: u32::try_from(size_of::<T>()).unwrap_or(u32::MAX),
        version: ABI_VERSION,
    }
}
fn valid_header<T>(header: StitchHeader) -> Result<(), StitchResult> {
    if header.version != ABI_VERSION {
        return Err(StitchResult::UnsupportedVersion);
    }
    if header.struct_size != u32::try_from(size_of::<T>()).unwrap_or(u32::MAX) {
        return Err(StitchResult::MalformedStruct);
    }
    Ok(())
}
fn digest(hash: Option<RevisionHash>) -> StitchDigest {
    StitchDigest {
        bytes: hash.map_or([0; 32], |value| value.0),
    }
}
fn detail(out: *mut StitchResultDetail, result: StitchResult) {
    if !out.is_null() {
        // SAFETY: every exported function documents that non-null output pointers target writable POD storage.
        unsafe {
            *out = StitchResultDetail {
                header: header::<StitchResultDetail>(),
                result,
                domain: if result == StitchResult::ModelError {
                    2
                } else {
                    1
                },
                diagnostic: result as u64,
            };
        }
    }
}
fn writable<T>(pointer: *mut T) -> Result<(), StitchResult> {
    if pointer.is_null() {
        Err(StitchResult::InvalidArgument)
    } else {
        Ok(())
    }
}
fn ffi(out: *mut StitchResultDetail, operation: impl FnOnce() -> StitchResult) -> StitchResult {
    let result = catch_unwind(AssertUnwindSafe(operation)).unwrap_or(StitchResult::Panic);
    detail(out, result);
    result
}
fn ffi_try(
    out: *mut StitchResultDetail,
    operation: impl FnOnce() -> Result<(), StitchResult>,
) -> StitchResult {
    ffi(out, || match operation() {
        Ok(()) => StitchResult::Ok,
        Err(error) => error,
    })
}
unsafe fn input<'a, T>(pointer: *const T) -> Result<&'a T, StitchResult> {
    if pointer.is_null() {
        return Err(StitchResult::InvalidArgument);
    }
    // SAFETY: callers provide a valid, properly aligned allocation for the declared POD type.
    Ok(unsafe { &*pointer })
}
unsafe fn output<T>(pointer: *mut T, value: T) -> Result<(), StitchResult> {
    if pointer.is_null() {
        return Err(StitchResult::InvalidArgument);
    }
    // SAFETY: callers provide a valid, properly aligned writable allocation for the declared POD type.
    unsafe {
        *pointer = value;
    }
    Ok(())
}
fn core(handle: u64) -> Result<Arc<Mutex<CoreState>>, StitchResult> {
    cores()
        .lock()
        .map_err(|_| StitchResult::Panic)?
        .get(&handle)
        .cloned()
        .ok_or(StitchResult::UnknownHandle)
}
fn rational(value: StitchRational) -> Result<Rational, StitchResult> {
    Rational::new(value.numerator, value.denominator).map_err(|_| StitchResult::BadRational)
}
fn range(value: StitchTimeRange) -> Result<TimeRange, StitchResult> {
    TimeRange::new(rational(value.start)?, rational(value.end)?)
        .map_err(|_| StitchResult::BadRational)
}

fn zero_id(value: StitchId) -> bool {
    value.bytes == [0; 16]
}

fn zero_digest(value: StitchDigest) -> bool {
    value.bytes == [0; 32]
}

fn zero_range(value: StitchTimeRange) -> bool {
    value.start.numerator == 0
        && value.start.denominator == 0
        && value.end.numerator == 0
        && value.end.denominator == 0
}

fn command(value: StitchCommand) -> Result<Command, StitchResult> {
    valid_header::<StitchCommand>(value.header)?;
    if value.reserved != 0 {
        return Err(StitchResult::InvalidArgument);
    }
    match value.kind {
        1 => {
            if !zero_id(value.secondary_id)
                || !zero_id(value.tertiary_id)
                || !zero_range(value.timeline_range)
                || !zero_range(value.source_range)
            {
                return Err(StitchResult::InvalidArgument);
            }
            Ok(Command::AddAsset {
                asset: Asset {
                    id: AssetId(value.primary_id.bytes),
                    identity: AssetIdentity {
                        content_sha256: value.content_sha256.bytes,
                        byte_length: value.byte_length,
                        provider_id: value.provider_id.bytes,
                    },
                },
            })
        }
        2 => {
            if !zero_id(value.secondary_id)
                || !zero_id(value.tertiary_id)
                || !zero_digest(value.content_sha256)
                || value.byte_length != 0
                || !zero_id(value.provider_id)
                || !zero_range(value.timeline_range)
                || !zero_range(value.source_range)
            {
                return Err(StitchResult::InvalidArgument);
            }
            Ok(Command::AddTrack {
                track_id: TrackId(value.primary_id.bytes),
            })
        }
        3 => {
            if !zero_digest(value.content_sha256)
                || value.byte_length != 0
                || !zero_id(value.provider_id)
            {
                return Err(StitchResult::InvalidArgument);
            }
            Ok(Command::AddClip {
                clip_id: ClipId(value.primary_id.bytes),
                asset_id: AssetId(value.secondary_id.bytes),
                track_id: TrackId(value.tertiary_id.bytes),
                timeline_range: range(value.timeline_range)?,
                source_range: range(value.source_range)?,
            })
        }
        4 => {
            if !zero_id(value.secondary_id)
                || !zero_id(value.tertiary_id)
                || !zero_digest(value.content_sha256)
                || value.byte_length != 0
                || !zero_id(value.provider_id)
                || !zero_range(value.source_range)
            {
                return Err(StitchResult::InvalidArgument);
            }
            Ok(Command::MoveClip {
                clip_id: ClipId(value.primary_id.bytes),
                timeline_range: range(value.timeline_range)?,
            })
        }
        5 => {
            if !zero_id(value.secondary_id)
                || !zero_id(value.tertiary_id)
                || !zero_digest(value.content_sha256)
                || value.byte_length != 0
                || !zero_id(value.provider_id)
                || !zero_range(value.timeline_range)
                || !zero_range(value.source_range)
            {
                return Err(StitchResult::InvalidArgument);
            }
            Ok(Command::RemoveClip {
                clip_id: ClipId(value.primary_id.bytes),
            })
        }
        _ => Err(StitchResult::BadEnum),
    }
}
fn snapshot_metadata(state: &CoreState) -> StitchSnapshotMetadata {
    let project = state.history.project();
    StitchSnapshotMetadata {
        header: header::<StitchSnapshotMetadata>(),
        revision: digest(state.history.current_hash()),
        project_digest: StitchDigest {
            bytes: project.semantic_digest().0,
        },
        epoch: state.epoch,
        asset_count: project.asset_count() as u64,
        track_count: project.track_count() as u64,
        clip_count: project.clip_count() as u64,
    }
}
struct CallbackGuard;
impl CallbackGuard {
    fn enter() -> Self {
        IN_CALLBACK.with(|flag| flag.set(true));
        Self
    }
}
impl Drop for CallbackGuard {
    fn drop(&mut self) {
        IN_CALLBACK.with(|flag| flag.set(false));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn stitch_abi_version() -> u32 {
    ABI_VERSION
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stitch_abi_negotiate(
    in_out: *mut StitchAbiNegotiation,
    out: *mut StitchResultDetail,
) -> StitchResult {
    ffi_try(out, || unsafe {
        let value = input(in_out)?;
        valid_header::<StitchAbiNegotiation>(value.header)?;
        if value.requested_major != ABI_VERSION || value.requested_minor > 0 {
            return Err(StitchResult::UnsupportedVersion);
        }
        output(
            in_out,
            StitchAbiNegotiation {
                header: header::<StitchAbiNegotiation>(),
                requested_major: value.requested_major,
                requested_minor: value.requested_minor,
                supported_major: ABI_VERSION,
                supported_minor: 0,
            },
        )
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn stitch_core_create(
    config: *const StitchCoreConfig,
    out_core: *mut u64,
    out: *mut StitchResultDetail,
) -> StitchResult {
    ffi_try(out, || unsafe {
        writable(out_core)?;
        let config = input(config)?;
        valid_header::<StitchCoreConfig>(config.header)?;
        let handle = next_handle();
        cores().lock().map_err(|_| StitchResult::Panic)?.insert(
            handle,
            Arc::new(Mutex::new(CoreState {
                history: EditorHistory::default(),
                epoch: config.initial_epoch,
                leases: HashMap::new(),
                owner: std::thread::current().id(),
            })),
        );
        output(out_core, handle)?;
        Ok(())
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stitch_core_destroy(
    handle: u64,
    out: *mut StitchResultDetail,
) -> StitchResult {
    ffi(out, || {
        if IN_CALLBACK.with(Cell::get) {
            return StitchResult::ReentrantMutation;
        }
        cores()
            .lock()
            .map_err(|_| StitchResult::Panic)
            .and_then(|mut entries| {
                let Some(core) = entries.get(&handle) else {
                    return Err(StitchResult::UnknownHandle);
                };
                let state = core.lock().map_err(|_| StitchResult::Panic)?;
                if state.owner != std::thread::current().id() {
                    return Err(StitchResult::InvalidArgument);
                }
                if !state.leases.is_empty() {
                    return Err(StitchResult::BadLeaseState);
                }
                drop(state);
                entries.remove(&handle);
                Ok(StitchResult::Ok)
            })
            .unwrap_or_else(|error| error)
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stitch_core_set_epoch(
    handle: u64,
    epoch: u64,
    out: *mut StitchResultDetail,
) -> StitchResult {
    ffi(out, || {
        if IN_CALLBACK.with(Cell::get) {
            return StitchResult::ReentrantMutation;
        }
        let Ok(core) = core(handle) else {
            return StitchResult::UnknownHandle;
        };
        let Ok(mut state) = core.lock() else {
            return StitchResult::Panic;
        };
        if state.owner != std::thread::current().id() {
            return StitchResult::InvalidArgument;
        }
        if epoch < state.epoch {
            return StitchResult::StaleEpoch;
        }
        state.epoch = epoch;
        StitchResult::Ok
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stitch_core_snapshot(
    handle: u64,
    out_snapshot: *mut u64,
    out: *mut StitchResultDetail,
) -> StitchResult {
    ffi_try(out, || unsafe {
        writable(out_snapshot)?;
        let entry = core(handle)?;
        let state = entry.lock().map_err(|_| StitchResult::Panic)?;
        let id = next_handle();
        snapshots().lock().map_err(|_| StitchResult::Panic)?.insert(
            id,
            RefHandle {
                value: snapshot_metadata(&state),
                refs: 1,
            },
        );
        output(out_snapshot, id)?;
        Ok(())
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn stitch_core_submit(
    core_handle: u64,
    batch: *const StitchCommandBatch,
    callback: StitchDiffCallback,
    context: *mut core::ffi::c_void,
    out_diff: *mut u64,
    out: *mut StitchResultDetail,
) -> StitchResult {
    ffi_try(out, || unsafe {
        writable(out_diff)?;
        if IN_CALLBACK.with(Cell::get) {
            return Err(StitchResult::ReentrantMutation);
        }
        let batch = input(batch)?;
        valid_header::<StitchCommandBatch>(batch.header)?;
        if batch.reserved != 0 {
            return Err(StitchResult::InvalidBatch);
        }
        let count = usize::try_from(batch.command_count).map_err(|_| StitchResult::InvalidBatch)?;
        if count == 0 || count > MAX_COMMANDS || (count > 0 && batch.commands.is_null()) {
            return Err(StitchResult::InvalidBatch);
        }
        // SAFETY: validated callers provide a live allocation containing exactly command_count POD entries.
        let commands = core::slice::from_raw_parts(batch.commands, count);
        let entry = core(core_handle)?;
        let mut state = entry.lock().map_err(|_| StitchResult::Panic)?;
        if state.owner != std::thread::current().id() {
            return Err(StitchResult::InvalidArgument);
        }
        if batch.epoch != state.epoch {
            return Err(StitchResult::StaleEpoch);
        }
        if digest(state.history.current_hash()) != batch.expected_revision {
            return Err(StitchResult::StaleRevision);
        }
        let previous = digest(state.history.current_hash());
        let mut next = state.history.clone();
        for raw in commands {
            next.apply(&command(*raw)?)
                .map_err(|_| StitchResult::ModelError)?;
        }
        state.history = next;
        let metadata = StitchDiffMetadata {
            header: header::<StitchDiffMetadata>(),
            previous_revision: previous,
            revision: digest(state.history.current_hash()),
            project_digest: StitchDigest {
                bytes: state.history.project().semantic_digest().0,
            },
            epoch: state.epoch,
        };
        drop(state);
        let id = next_handle();
        diffs().lock().map_err(|_| StitchResult::Panic)?.insert(
            id,
            RefHandle {
                value: metadata,
                refs: 1,
            },
        );
        output(out_diff, id)?;
        if let Some(callback) = callback {
            let _guard = CallbackGuard::enter();
            callback(context, id);
        }
        Ok(())
    })
}
fn retain<T>(entries: &Mutex<HashMap<u64, RefHandle<T>>>, handle: u64) -> StitchResult {
    let Ok(mut entries) = entries.lock() else {
        return StitchResult::Panic;
    };
    let Some(entry) = entries.get_mut(&handle) else {
        return StitchResult::UnknownHandle;
    };
    let Some(next) = entry.refs.checked_add(1) else {
        return StitchResult::InvalidArgument;
    };
    entry.refs = next;
    StitchResult::Ok
}
fn release<T>(entries: &Mutex<HashMap<u64, RefHandle<T>>>, handle: u64) -> StitchResult {
    let Ok(mut entries) = entries.lock() else {
        return StitchResult::Panic;
    };
    let Some(entry) = entries.get_mut(&handle) else {
        return StitchResult::UnknownHandle;
    };
    if entry.refs == 1 {
        entries.remove(&handle);
    } else {
        entry.refs -= 1;
    }
    StitchResult::Ok
}
#[unsafe(no_mangle)]
pub extern "C" fn stitch_snapshot_retain(
    handle: u64,
    out: *mut StitchResultDetail,
) -> StitchResult {
    ffi(out, || retain(snapshots(), handle))
}
#[unsafe(no_mangle)]
pub extern "C" fn stitch_snapshot_release(
    handle: u64,
    out: *mut StitchResultDetail,
) -> StitchResult {
    ffi(out, || release(snapshots(), handle))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stitch_snapshot_metadata_get(
    handle: u64,
    out_metadata: *mut StitchSnapshotMetadata,
    out: *mut StitchResultDetail,
) -> StitchResult {
    ffi_try(out, || unsafe {
        let entries = snapshots().lock().map_err(|_| StitchResult::Panic)?;
        let value = entries
            .get(&handle)
            .ok_or(StitchResult::UnknownHandle)?
            .value;
        output(out_metadata, value)
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn stitch_diff_retain(handle: u64, out: *mut StitchResultDetail) -> StitchResult {
    ffi(out, || retain(diffs(), handle))
}
#[unsafe(no_mangle)]
pub extern "C" fn stitch_diff_release(handle: u64, out: *mut StitchResultDetail) -> StitchResult {
    ffi(out, || release(diffs(), handle))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stitch_diff_metadata_get(
    handle: u64,
    out_metadata: *mut StitchDiffMetadata,
    out: *mut StitchResultDetail,
) -> StitchResult {
    ffi_try(out, || unsafe {
        let entries = diffs().lock().map_err(|_| StitchResult::Panic)?;
        let value = entries
            .get(&handle)
            .ok_or(StitchResult::UnknownHandle)?
            .value;
        output(out_metadata, value)
    })
}

fn lease_transition(
    core_handle: u64,
    lease_id: u64,
    device: u64,
    expected: LeaseState,
    next: LeaseState,
) -> StitchResult {
    let Ok(entry) = core(core_handle) else {
        return StitchResult::UnknownHandle;
    };
    let Ok(mut state) = entry.lock() else {
        return StitchResult::Panic;
    };
    let Some(lease) = state.leases.get_mut(&lease_id) else {
        return StitchResult::UnknownHandle;
    };
    if lease.metadata.device_generation != device {
        return StitchResult::StaleDevice;
    }
    if lease.owner != std::thread::current().id() {
        return StitchResult::InvalidArgument;
    }
    if lease.state != expected {
        return StitchResult::BadLeaseState;
    }
    lease.state = next;
    lease.metadata.state = next as u32;
    StitchResult::Ok
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stitch_frame_lease_register(
    core_handle: u64,
    metadata: *const StitchFrameMetadata,
    out_lease: *mut u64,
    out: *mut StitchResultDetail,
) -> StitchResult {
    ffi_try(out, || unsafe {
        if IN_CALLBACK.with(Cell::get) {
            return Err(StitchResult::ReentrantMutation);
        }
        writable(out_lease)?;
        let metadata = input(metadata)?;
        valid_header::<StitchFrameMetadata>(metadata.header)?;
        rational(metadata.presentation_time)?;
        if metadata.lease_id != 0
            || metadata.state != LeaseState::Registered as u32
            || metadata.device_generation == 0
            || metadata.surface_token == 0
            || metadata.pixel_width == 0
            || metadata.pixel_height == 0
            || metadata.pixel_format != 1
            || !matches!(metadata.synchronization_kind, 1 | 2)
            || (metadata.synchronization_kind == 2 && metadata.synchronization_token == 0)
            || metadata.owner_thread_token != 0
            || metadata.reserved != 0
        {
            return Err(StitchResult::InvalidArgument);
        }
        let entry = core(core_handle)?;
        let mut state = entry.lock().map_err(|_| StitchResult::Panic)?;
        if metadata.epoch != state.epoch {
            return Err(StitchResult::StaleEpoch);
        }
        if metadata.revision != digest(state.history.current_hash()) {
            return Err(StitchResult::StaleRevision);
        }
        let id = next_handle();
        let mut value = *metadata;
        value.lease_id = id;
        value.owner_thread_token = id;
        state.leases.insert(
            id,
            Lease {
                metadata: value,
                state: LeaseState::Registered,
                owner: std::thread::current().id(),
            },
        );
        output(out_lease, id)
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stitch_frame_lease_metadata_get(
    core_handle: u64,
    lease_id: u64,
    out_metadata: *mut StitchFrameMetadata,
    out: *mut StitchResultDetail,
) -> StitchResult {
    ffi_try(out, || unsafe {
        let entry = core(core_handle)?;
        let state = entry.lock().map_err(|_| StitchResult::Panic)?;
        output(
            out_metadata,
            state
                .leases
                .get(&lease_id)
                .ok_or(StitchResult::UnknownHandle)?
                .metadata,
        )
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stitch_frame_lease_acquire(
    core_handle: u64,
    lease_id: u64,
    device: u64,
    epoch: u64,
    revision: *const StitchDigest,
    out: *mut StitchResultDetail,
) -> StitchResult {
    ffi_try(out, || unsafe {
        if IN_CALLBACK.with(Cell::get) {
            return Err(StitchResult::ReentrantMutation);
        }
        let revision = input(revision)?;
        let entry = core(core_handle)?;
        {
            let state = entry.lock().map_err(|_| StitchResult::Panic)?;
            if epoch != state.epoch {
                return Err(StitchResult::StaleEpoch);
            }
            if *revision != digest(state.history.current_hash()) {
                return Err(StitchResult::StaleRevision);
            }
        }
        match lease_transition(
            core_handle,
            lease_id,
            device,
            LeaseState::Registered,
            LeaseState::Acquired,
        ) {
            StitchResult::Ok => Ok(()),
            error => Err(error),
        }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn stitch_frame_lease_discard(
    core_handle: u64,
    lease_id: u64,
    device: u64,
    out: *mut StitchResultDetail,
) -> StitchResult {
    ffi(out, || {
        if IN_CALLBACK.with(Cell::get) {
            return StitchResult::ReentrantMutation;
        }
        let Ok(entry) = core(core_handle) else {
            return StitchResult::UnknownHandle;
        };
        let Ok(mut state) = entry.lock() else {
            return StitchResult::Panic;
        };
        let Some(lease) = state.leases.get(&lease_id) else {
            return StitchResult::UnknownHandle;
        };
        if lease.metadata.device_generation != device {
            return StitchResult::StaleDevice;
        }
        if lease.owner != std::thread::current().id() {
            return StitchResult::InvalidArgument;
        }
        if !matches!(lease.state, LeaseState::Registered | LeaseState::Acquired) {
            return StitchResult::BadLeaseState;
        }
        state.leases.remove(&lease_id);
        StitchResult::Ok
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn stitch_frame_lease_submit(
    core_handle: u64,
    lease: u64,
    device: u64,
    out: *mut StitchResultDetail,
) -> StitchResult {
    ffi(out, || {
        if IN_CALLBACK.with(Cell::get) {
            return StitchResult::ReentrantMutation;
        }
        let Ok(entry) = core(core_handle) else {
            return StitchResult::UnknownHandle;
        };
        let Ok(state) = entry.lock() else {
            return StitchResult::Panic;
        };
        let Some(current) = state.leases.get(&lease) else {
            return StitchResult::UnknownHandle;
        };
        if current.metadata.epoch != state.epoch {
            return StitchResult::StaleEpoch;
        }
        if current.metadata.revision != digest(state.history.current_hash()) {
            return StitchResult::StaleRevision;
        }
        drop(state);
        lease_transition(
            core_handle,
            lease,
            device,
            LeaseState::Acquired,
            LeaseState::Submitted,
        )
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn stitch_frame_lease_complete(
    core: u64,
    lease: u64,
    device: u64,
    out: *mut StitchResultDetail,
) -> StitchResult {
    ffi(out, || {
        if IN_CALLBACK.with(Cell::get) {
            return StitchResult::ReentrantMutation;
        }
        lease_transition(
            core,
            lease,
            device,
            LeaseState::Submitted,
            LeaseState::Completed,
        )
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn stitch_frame_lease_retire(
    core_handle: u64,
    lease: u64,
    device: u64,
    out: *mut StitchResultDetail,
) -> StitchResult {
    ffi(out, || {
        if IN_CALLBACK.with(Cell::get) {
            return StitchResult::ReentrantMutation;
        }
        let result = lease_transition(
            core_handle,
            lease,
            device,
            LeaseState::Completed,
            LeaseState::Retired,
        );
        if result != StitchResult::Ok {
            return result;
        }
        let Ok(entry) = core(core_handle) else {
            return StitchResult::UnknownHandle;
        };
        let Ok(mut state) = entry.lock() else {
            return StitchResult::Panic;
        };
        state.leases.remove(&lease);
        StitchResult::Ok
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> StitchCoreConfig {
        StitchCoreConfig {
            header: header::<StitchCoreConfig>(),
            initial_epoch: 0,
        }
    }
    fn command(kind: u32) -> StitchCommand {
        let empty_range = StitchTimeRange {
            start: StitchRational {
                numerator: 0,
                denominator: 0,
            },
            end: StitchRational {
                numerator: 0,
                denominator: 0,
            },
        };
        let used_range = StitchTimeRange {
            start: StitchRational {
                numerator: 0,
                denominator: 1,
            },
            end: StitchRational {
                numerator: 1,
                denominator: 1,
            },
        };
        StitchCommand {
            header: header::<StitchCommand>(),
            kind,
            reserved: 0,
            primary_id: StitchId { bytes: [1; 16] },
            secondary_id: StitchId::default(),
            tertiary_id: StitchId::default(),
            content_sha256: StitchDigest::default(),
            byte_length: 0,
            provider_id: StitchId::default(),
            timeline_range: if matches!(kind, 3 | 4) {
                used_range
            } else {
                empty_range
            },
            source_range: if kind == 3 { used_range } else { empty_range },
        }
    }

    #[test]
    fn version_lifecycle_batch_diff_and_null_outputs_are_typed() {
        let mut negotiation = StitchAbiNegotiation {
            header: header::<StitchAbiNegotiation>(),
            requested_major: ABI_VERSION,
            requested_minor: 0,
            supported_major: 0,
            supported_minor: 0,
        };
        assert_eq!(
            unsafe { stitch_abi_negotiate(&raw mut negotiation, core::ptr::null_mut()) },
            StitchResult::Ok
        );
        let mut core = 0;
        assert_eq!(
            unsafe { stitch_core_create(&config(), &raw mut core, core::ptr::null_mut()) },
            StitchResult::Ok
        );
        assert_ne!(core, 0);
        assert_eq!(
            unsafe { stitch_core_snapshot(core, core::ptr::null_mut(), core::ptr::null_mut()) },
            StitchResult::InvalidArgument
        );
        let raw = command(2);
        let batch = StitchCommandBatch {
            header: header::<StitchCommandBatch>(),
            commands: &raw const raw,
            command_count: 1,
            reserved: 0,
            expected_revision: StitchDigest::default(),
            epoch: 0,
        };
        let mut diff = 0;
        assert_eq!(
            unsafe {
                stitch_core_submit(
                    core,
                    &raw const batch,
                    None,
                    core::ptr::null_mut(),
                    &raw mut diff,
                    core::ptr::null_mut(),
                )
            },
            StitchResult::Ok
        );
        let mut metadata = StitchDiffMetadata {
            header: header::<StitchDiffMetadata>(),
            previous_revision: StitchDigest::default(),
            revision: StitchDigest::default(),
            project_digest: StitchDigest::default(),
            epoch: 0,
        };
        assert_eq!(
            unsafe { stitch_diff_metadata_get(diff, &raw mut metadata, core::ptr::null_mut()) },
            StitchResult::Ok
        );
        assert_eq!(
            stitch_diff_release(diff, core::ptr::null_mut()),
            StitchResult::Ok
        );
        assert_eq!(
            unsafe { stitch_core_destroy(core, core::ptr::null_mut()) },
            StitchResult::Ok
        );
        for _ in 0..1_000 {
            let mut handle = 0;
            assert_eq!(
                unsafe { stitch_core_create(&config(), &raw mut handle, core::ptr::null_mut()) },
                StitchResult::Ok
            );
            assert_eq!(
                unsafe { stitch_core_destroy(handle, core::ptr::null_mut()) },
                StitchResult::Ok
            );
        }
    }

    #[test]
    fn command_reserved_and_unused_fields_are_rejected() {
        let mut core = 0;
        assert_eq!(
            unsafe { stitch_core_create(&config(), &raw mut core, core::ptr::null_mut()) },
            StitchResult::Ok
        );

        let mut raw = command(2);
        raw.reserved = 1;
        let mut batch = StitchCommandBatch {
            header: header::<StitchCommandBatch>(),
            commands: &raw const raw,
            command_count: 1,
            reserved: 0,
            expected_revision: StitchDigest::default(),
            epoch: 0,
        };
        let mut diff = 0;
        assert_eq!(
            unsafe {
                stitch_core_submit(
                    core,
                    &raw const batch,
                    None,
                    core::ptr::null_mut(),
                    &raw mut diff,
                    core::ptr::null_mut(),
                )
            },
            StitchResult::InvalidArgument
        );

        raw = command(2);
        raw.content_sha256.bytes[0] = 1;
        batch.commands = &raw const raw;
        assert_eq!(
            unsafe {
                stitch_core_submit(
                    core,
                    &raw const batch,
                    None,
                    core::ptr::null_mut(),
                    &raw mut diff,
                    core::ptr::null_mut(),
                )
            },
            StitchResult::InvalidArgument
        );

        raw = command(2);
        batch.commands = &raw const raw;
        batch.reserved = 1;
        assert_eq!(
            unsafe {
                stitch_core_submit(
                    core,
                    &raw const batch,
                    None,
                    core::ptr::null_mut(),
                    &raw mut diff,
                    core::ptr::null_mut(),
                )
            },
            StitchResult::InvalidBatch
        );
        assert_eq!(
            unsafe { stitch_core_destroy(core, core::ptr::null_mut()) },
            StitchResult::Ok
        );
    }

    #[test]
    fn malformed_retain_panic_and_adversarial_batches_are_typed() {
        assert_eq!(
            ffi(core::ptr::null_mut(), || panic!("injected")),
            StitchResult::Panic
        );
        let id = next_handle();
        snapshots().lock().unwrap().insert(
            id,
            RefHandle {
                value: StitchSnapshotMetadata {
                    header: header::<StitchSnapshotMetadata>(),
                    revision: StitchDigest::default(),
                    project_digest: StitchDigest::default(),
                    epoch: 0,
                    asset_count: 0,
                    track_count: 0,
                    clip_count: 0,
                },
                refs: u32::MAX,
            },
        );
        assert_eq!(retain(snapshots(), id), StitchResult::InvalidArgument);
        assert_eq!(snapshots().lock().unwrap().get(&id).unwrap().refs, u32::MAX);
        snapshots().lock().unwrap().remove(&id);
        let mut core = 0;
        assert_eq!(
            unsafe { stitch_core_create(&config(), &raw mut core, core::ptr::null_mut()) },
            StitchResult::Ok
        );
        for kind in 0..10_000_u32 {
            let mut raw = command(kind % 7);
            raw.header.struct_size = if kind % 2 == 0 {
                0
            } else {
                header::<StitchCommand>().struct_size
            };
            let batch = StitchCommandBatch {
                header: header::<StitchCommandBatch>(),
                commands: &raw const raw,
                command_count: 1,
                reserved: 0,
                expected_revision: StitchDigest::default(),
                epoch: 0,
            };
            let mut diff = 0;
            let result = unsafe {
                stitch_core_submit(
                    core,
                    &raw const batch,
                    None,
                    core::ptr::null_mut(),
                    &raw mut diff,
                    core::ptr::null_mut(),
                )
            };
            assert!(matches!(
                result,
                StitchResult::MalformedStruct
                    | StitchResult::InvalidArgument
                    | StitchResult::BadEnum
                    | StitchResult::ModelError
                    | StitchResult::Ok
                    | StitchResult::StaleRevision
            ));
            if result == StitchResult::Ok {
                assert_eq!(
                    stitch_diff_release(diff, core::ptr::null_mut()),
                    StitchResult::Ok
                );
            }
        }
        assert_eq!(
            unsafe { stitch_core_destroy(core, core::ptr::null_mut()) },
            StitchResult::Ok
        );
    }

    struct CallbackState {
        core: u64,
        callback_thread: Option<std::thread::ThreadId>,
        snapshot: StitchResult,
        snapshot_metadata: StitchResult,
        set_epoch: StitchResult,
        submit: StitchResult,
        destroy: StitchResult,
        frame_register: StitchResult,
    }

    extern "C" fn callback(context: *mut core::ffi::c_void, _diff: u64) {
        // SAFETY: the test passes a valid CallbackState for the synchronous callback duration.
        let state = unsafe { &mut *context.cast::<CallbackState>() };
        let mut snapshot = 0;
        let mut metadata = StitchSnapshotMetadata {
            header: header::<StitchSnapshotMetadata>(),
            revision: StitchDigest::default(),
            project_digest: StitchDigest::default(),
            epoch: 0,
            asset_count: 0,
            track_count: 0,
            clip_count: 0,
        };
        let mut reentrant_diff = 0;
        state.callback_thread = Some(std::thread::current().id());
        state.snapshot =
            unsafe { stitch_core_snapshot(state.core, &raw mut snapshot, core::ptr::null_mut()) };
        state.snapshot_metadata = if state.snapshot == StitchResult::Ok {
            unsafe {
                stitch_snapshot_metadata_get(snapshot, &raw mut metadata, core::ptr::null_mut())
            }
        } else {
            state.snapshot
        };
        state.set_epoch = unsafe { stitch_core_set_epoch(state.core, 1, core::ptr::null_mut()) };
        state.submit = unsafe {
            stitch_core_submit(
                state.core,
                core::ptr::null(),
                None,
                core::ptr::null_mut(),
                &raw mut reentrant_diff,
                core::ptr::null_mut(),
            )
        };
        state.destroy = unsafe { stitch_core_destroy(state.core, core::ptr::null_mut()) };
        let frame = frame(metadata.revision);
        let mut lease = 0;
        state.frame_register = unsafe {
            stitch_frame_lease_register(
                state.core,
                &raw const frame,
                &raw mut lease,
                core::ptr::null_mut(),
            )
        };
        if state.snapshot == StitchResult::Ok {
            let _ = stitch_snapshot_release(snapshot, core::ptr::null_mut());
        }
    }

    #[test]
    fn callback_allows_read_only_and_rejects_mutation_and_destroy() {
        let mut core = 0;
        assert_eq!(
            unsafe { stitch_core_create(&config(), &raw mut core, core::ptr::null_mut()) },
            StitchResult::Ok
        );
        let raw = command(2);
        let batch = StitchCommandBatch {
            header: header::<StitchCommandBatch>(),
            commands: &raw const raw,
            command_count: 1,
            reserved: 0,
            expected_revision: StitchDigest::default(),
            epoch: 0,
        };
        let caller_thread = std::thread::current().id();
        let mut state = CallbackState {
            core,
            callback_thread: None,
            snapshot: StitchResult::Panic,
            snapshot_metadata: StitchResult::Panic,
            set_epoch: StitchResult::Ok,
            submit: StitchResult::Ok,
            destroy: StitchResult::Ok,
            frame_register: StitchResult::Ok,
        };
        let mut diff = 0;
        assert_eq!(
            unsafe {
                stitch_core_submit(
                    core,
                    &raw const batch,
                    Some(callback),
                    (&raw mut state).cast(),
                    &raw mut diff,
                    core::ptr::null_mut(),
                )
            },
            StitchResult::Ok
        );
        assert_eq!(state.callback_thread, Some(caller_thread));
        assert_eq!(state.snapshot, StitchResult::Ok);
        assert_eq!(state.snapshot_metadata, StitchResult::Ok);
        assert_eq!(state.set_epoch, StitchResult::ReentrantMutation);
        assert_eq!(state.submit, StitchResult::ReentrantMutation);
        assert_eq!(state.destroy, StitchResult::ReentrantMutation);
        assert_eq!(state.frame_register, StitchResult::ReentrantMutation);
        assert_eq!(
            stitch_diff_release(diff, core::ptr::null_mut()),
            StitchResult::Ok
        );
        assert_eq!(
            unsafe { stitch_core_destroy(core, core::ptr::null_mut()) },
            StitchResult::Ok
        );
    }

    fn frame(revision: StitchDigest) -> StitchFrameMetadata {
        StitchFrameMetadata {
            header: header::<StitchFrameMetadata>(),
            lease_id: 0,
            revision,
            epoch: 0,
            device_generation: 7,
            surface_token: 9,
            synchronization_token: 10,
            owner_thread_token: 0,
            pixel_width: 16,
            pixel_height: 16,
            pixel_format: 1,
            synchronization_kind: 2,
            synchronization_value: 1,
            presentation_time: StitchRational {
                numerator: 0,
                denominator: 1,
            },
            state: LeaseState::Registered as u32,
            reserved: 0,
        }
    }

    fn acquire_frame(
        core: u64,
        lease: u64,
        device: u64,
        epoch: u64,
        revision: &StitchDigest,
    ) -> StitchResult {
        unsafe {
            stitch_frame_lease_acquire(core, lease, device, epoch, revision, core::ptr::null_mut())
        }
    }

    fn core_with_track() -> (u64, u64, StitchDiffMetadata) {
        let mut core = 0;
        assert_eq!(
            unsafe { stitch_core_create(&config(), &raw mut core, core::ptr::null_mut()) },
            StitchResult::Ok
        );
        let raw = command(2);
        let batch = StitchCommandBatch {
            header: header::<StitchCommandBatch>(),
            commands: &raw const raw,
            command_count: 1,
            reserved: 0,
            expected_revision: StitchDigest::default(),
            epoch: 0,
        };
        let mut diff = 0;
        assert_eq!(
            unsafe {
                stitch_core_submit(
                    core,
                    &raw const batch,
                    None,
                    core::ptr::null_mut(),
                    &raw mut diff,
                    core::ptr::null_mut(),
                )
            },
            StitchResult::Ok
        );
        let mut metadata = StitchDiffMetadata {
            header: header::<StitchDiffMetadata>(),
            previous_revision: StitchDigest::default(),
            revision: StitchDigest::default(),
            project_digest: StitchDigest::default(),
            epoch: 0,
        };
        assert_eq!(
            unsafe { stitch_diff_metadata_get(diff, &raw mut metadata, core::ptr::null_mut()) },
            StitchResult::Ok
        );
        (core, diff, metadata)
    }

    fn register_frame(core: u64, revision: StitchDigest) -> u64 {
        let input = frame(revision);
        let mut lease = 0;
        assert_eq!(
            unsafe {
                stitch_frame_lease_register(
                    core,
                    &raw const input,
                    &raw mut lease,
                    core::ptr::null_mut(),
                )
            },
            StitchResult::Ok
        );
        lease
    }

    fn lease_metadata_result(core: u64, lease: u64, revision: StitchDigest) -> StitchResult {
        let mut metadata = frame(revision);
        unsafe {
            stitch_frame_lease_metadata_get(core, lease, &raw mut metadata, core::ptr::null_mut())
        }
    }

    #[test]
    fn lease_lifecycle_rejects_stale_wrong_thread_and_removes_retired() {
        let (core, diff, metadata) = core_with_track();

        let mut malformed = frame(metadata.revision);
        malformed.surface_token = 0;
        let mut ignored_lease = 0;
        assert_eq!(
            unsafe {
                stitch_frame_lease_register(
                    core,
                    &raw const malformed,
                    &raw mut ignored_lease,
                    core::ptr::null_mut(),
                )
            },
            StitchResult::InvalidArgument
        );

        let input = frame(metadata.revision);
        let mut lease = 0;
        assert_eq!(
            unsafe {
                stitch_frame_lease_register(
                    core,
                    &raw const input,
                    &raw mut lease,
                    core::ptr::null_mut(),
                )
            },
            StitchResult::Ok
        );
        assert_eq!(
            unsafe { stitch_core_destroy(core, core::ptr::null_mut()) },
            StitchResult::BadLeaseState
        );
        assert_eq!(
            acquire_frame(core, lease, 8, 0, &metadata.revision),
            StitchResult::StaleDevice
        );
        let bad_revision = StitchDigest::default();
        assert_eq!(
            acquire_frame(core, lease, 7, 0, &bad_revision),
            StitchResult::StaleRevision
        );
        assert_eq!(
            acquire_frame(core, lease, 7, 1, &metadata.revision),
            StitchResult::StaleEpoch
        );
        assert_eq!(
            acquire_frame(core, lease, 7, 0, &metadata.revision),
            StitchResult::Ok
        );
        assert_eq!(
            stitch_frame_lease_submit(core, lease, 7, core::ptr::null_mut()),
            StitchResult::Ok
        );
        let wrong_thread = std::thread::spawn(move || {
            stitch_frame_lease_complete(core, lease, 7, core::ptr::null_mut())
        })
        .join()
        .unwrap();
        assert_eq!(wrong_thread, StitchResult::InvalidArgument);
        assert_eq!(
            unsafe { stitch_core_set_epoch(core, 1, core::ptr::null_mut()) },
            StitchResult::Ok
        );
        assert_eq!(
            stitch_frame_lease_submit(core, lease, 7, core::ptr::null_mut()),
            StitchResult::StaleEpoch
        );
        assert_eq!(
            stitch_frame_lease_complete(core, lease, 7, core::ptr::null_mut()),
            StitchResult::Ok
        );
        assert_eq!(
            stitch_frame_lease_retire(core, lease, 7, core::ptr::null_mut()),
            StitchResult::Ok
        );
        let mut out = frame(metadata.revision);
        assert_eq!(
            unsafe {
                stitch_frame_lease_metadata_get(core, lease, &raw mut out, core::ptr::null_mut())
            },
            StitchResult::UnknownHandle
        );
        assert_eq!(
            stitch_diff_release(diff, core::ptr::null_mut()),
            StitchResult::Ok
        );
        assert_eq!(
            unsafe { stitch_core_destroy(core, core::ptr::null_mut()) },
            StitchResult::Ok
        );
    }

    #[test]
    fn lease_discard_removes_registered_and_acquired_leases() {
        let (core, diff, metadata) = core_with_track();
        let registered = register_frame(core, metadata.revision);
        assert_eq!(
            stitch_frame_lease_discard(core, registered, 8, core::ptr::null_mut()),
            StitchResult::StaleDevice
        );
        let wrong_thread = std::thread::spawn(move || {
            stitch_frame_lease_discard(core, registered, 7, core::ptr::null_mut())
        })
        .join()
        .unwrap();
        assert_eq!(wrong_thread, StitchResult::InvalidArgument);
        assert_eq!(
            stitch_frame_lease_discard(core, registered, 7, core::ptr::null_mut()),
            StitchResult::Ok
        );
        assert_eq!(
            lease_metadata_result(core, registered, metadata.revision),
            StitchResult::UnknownHandle
        );

        let acquired = register_frame(core, metadata.revision);
        assert_eq!(
            acquire_frame(core, acquired, 7, 0, &metadata.revision),
            StitchResult::Ok
        );
        assert_eq!(
            stitch_frame_lease_discard(core, acquired, 7, core::ptr::null_mut()),
            StitchResult::Ok
        );
        assert_eq!(
            lease_metadata_result(core, acquired, metadata.revision),
            StitchResult::UnknownHandle
        );
        assert_eq!(
            stitch_diff_release(diff, core::ptr::null_mut()),
            StitchResult::Ok
        );
        assert_eq!(
            unsafe { stitch_core_destroy(core, core::ptr::null_mut()) },
            StitchResult::Ok
        );
    }

    #[test]
    fn lease_discard_rejects_submitted_work() {
        let (core, diff, metadata) = core_with_track();
        let lease = register_frame(core, metadata.revision);
        assert_eq!(
            acquire_frame(core, lease, 7, 0, &metadata.revision),
            StitchResult::Ok
        );
        assert_eq!(
            stitch_frame_lease_submit(core, lease, 7, core::ptr::null_mut()),
            StitchResult::Ok
        );
        assert_eq!(
            stitch_frame_lease_discard(core, lease, 7, core::ptr::null_mut()),
            StitchResult::BadLeaseState
        );
        assert_eq!(
            lease_metadata_result(core, lease, metadata.revision),
            StitchResult::Ok
        );
        assert_eq!(
            stitch_frame_lease_complete(core, lease, 7, core::ptr::null_mut()),
            StitchResult::Ok
        );
        assert_eq!(
            stitch_frame_lease_retire(core, lease, 7, core::ptr::null_mut()),
            StitchResult::Ok
        );
        assert_eq!(
            stitch_diff_release(diff, core::ptr::null_mut()),
            StitchResult::Ok
        );
        assert_eq!(
            unsafe { stitch_core_destroy(core, core::ptr::null_mut()) },
            StitchResult::Ok
        );
    }

    #[test]
    fn core_destroy_is_owner_thread_bound() {
        let mut core = 0;
        assert_eq!(
            unsafe { stitch_core_create(&config(), &raw mut core, core::ptr::null_mut()) },
            StitchResult::Ok
        );
        let from_other_thread =
            std::thread::spawn(move || unsafe { stitch_core_destroy(core, core::ptr::null_mut()) })
                .join()
                .unwrap();
        assert_eq!(from_other_thread, StitchResult::InvalidArgument);
        assert_eq!(
            unsafe { stitch_core_destroy(core, core::ptr::null_mut()) },
            StitchResult::Ok
        );
    }
}
