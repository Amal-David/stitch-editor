#ifndef STITCH_C_API_H
#define STITCH_C_API_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define STITCH_ABI_VERSION 1u
#define STITCH_ABI_MINOR_VERSION 0u
#define STITCH_DIGEST_BYTES 32u
#define STITCH_ID_BYTES 16u

typedef uint64_t stitch_core_handle;
typedef uint64_t stitch_snapshot_handle;
typedef uint64_t stitch_diff_handle;
typedef uint64_t stitch_frame_lease_id;
typedef uint64_t stitch_cancellation_epoch;
typedef uint64_t stitch_device_generation;
typedef uint32_t stitch_pixel_format;
typedef uint32_t stitch_synchronization_kind;
enum { STITCH_PIXEL_FORMAT_RGBA8_UNORM = 1 };
enum { STITCH_SYNCHRONIZATION_NONE = 1, STITCH_SYNCHRONIZATION_BRIDGE_TOKEN = 2 };

typedef uint32_t stitch_result;
enum {
  STITCH_OK = 0,
  STITCH_INVALID_ARGUMENT = 1,
  STITCH_UNSUPPORTED_VERSION = 2,
  STITCH_UNKNOWN_HANDLE = 3,
  STITCH_MALFORMED_STRUCT = 4,
  STITCH_BAD_ENUM = 5,
  STITCH_BAD_RATIONAL = 6,
  STITCH_INVALID_BATCH = 7,
  STITCH_STALE_REVISION = 8,
  STITCH_STALE_EPOCH = 9,
  STITCH_STALE_DEVICE = 10,
  STITCH_BAD_LEASE_STATE = 11,
  STITCH_REENTRANT_MUTATION = 12,
  STITCH_MODEL_ERROR = 13,
  STITCH_PANIC = 14
};

typedef struct stitch_header { uint32_t struct_size; uint32_t version; } stitch_header;
typedef struct stitch_digest { uint8_t bytes[STITCH_DIGEST_BYTES]; } stitch_digest;
typedef struct stitch_id { uint8_t bytes[STITCH_ID_BYTES]; } stitch_id;
typedef struct stitch_rational { int64_t numerator; int64_t denominator; } stitch_rational;
typedef struct stitch_time_range { stitch_rational start; stitch_rational end; } stitch_time_range;

typedef struct stitch_result_detail {
  stitch_header header;
  stitch_result result;
  uint32_t domain;
  uint64_t diagnostic;
} stitch_result_detail;
typedef struct stitch_abi_negotiation { stitch_header header; uint32_t requested_major; uint32_t requested_minor; uint32_t supported_major; uint32_t supported_minor; } stitch_abi_negotiation;

typedef struct stitch_core_config {
  stitch_header header;
  stitch_cancellation_epoch initial_epoch;
} stitch_core_config;

typedef uint32_t stitch_command_kind;
enum {
  STITCH_COMMAND_ADD_ASSET = 1,
  STITCH_COMMAND_ADD_TRACK = 2,
  STITCH_COMMAND_ADD_CLIP = 3,
  STITCH_COMMAND_MOVE_CLIP = 4,
  STITCH_COMMAND_REMOVE_CLIP = 5
};

/* A tagged, fixed-width command. Unused fields must be zero. */
typedef struct stitch_command {
  stitch_header header;
  uint32_t kind;
  uint32_t reserved;
  stitch_id primary_id;
  stitch_id secondary_id;
  stitch_id tertiary_id;
  stitch_digest content_sha256;
  uint64_t byte_length;
  stitch_id provider_id;
  stitch_time_range timeline_range;
  stitch_time_range source_range;
} stitch_command;

typedef struct stitch_command_batch {
  stitch_header header;
  const stitch_command *commands;
  uint32_t command_count;
  uint32_t reserved;
  stitch_digest expected_revision;
  stitch_cancellation_epoch epoch;
} stitch_command_batch;

typedef struct stitch_snapshot_metadata {
  stitch_header header;
  stitch_digest revision;
  stitch_digest project_digest;
  stitch_cancellation_epoch epoch;
  uint64_t asset_count;
  uint64_t track_count;
  uint64_t clip_count;
} stitch_snapshot_metadata;

typedef struct stitch_diff_metadata {
  stitch_header header;
  stitch_digest previous_revision;
  stitch_digest revision;
  stitch_digest project_digest;
  stitch_cancellation_epoch epoch;
} stitch_diff_metadata;

typedef uint32_t stitch_frame_lease_state;
enum {
  STITCH_LEASE_REGISTERED = 1,
  STITCH_LEASE_ACQUIRED = 2,
  STITCH_LEASE_SUBMITTED = 3,
  STITCH_LEASE_COMPLETED = 4,
  STITCH_LEASE_RETIRED = 5
};

typedef struct stitch_frame_metadata {
  stitch_header header;
  stitch_frame_lease_id lease_id;
  stitch_digest revision;
  stitch_cancellation_epoch epoch;
  stitch_device_generation device_generation;
  /* Opaque bridge-owned IDs, never native pointers or native-handle bit patterns. */
  uint64_t surface_token;
  uint64_t synchronization_token;
  uint64_t owner_thread_token;
  uint32_t pixel_width;
  uint32_t pixel_height;
  stitch_pixel_format pixel_format;
  stitch_synchronization_kind synchronization_kind;
  uint64_t synchronization_value;
  stitch_rational presentation_time;
  uint32_t state;
  uint32_t reserved;
} stitch_frame_metadata;

typedef void (*stitch_diff_callback)(void *context, stitch_diff_handle diff);

/*
 * ABI lifetime and threading contract
 * -----------------------------------
 * - All handles and tokens are nonzero opaque IDs. They are never pointers.
 * - out_detail is optional. Every other out parameter is required.
 * - Project-model mutation, cancellation-epoch mutation, and core destruction
 *   occur only on the thread that created the core. Frame-lease registration
 *   and transitions are the exception: they occur only on the bridge thread
 *   that registered that lease.
 * - Snapshot and diff handles returned through out parameters own one
 *   reference. retain adds a reference; each owned reference is released once.
 * - submit is synchronous. Its callback runs on the submitting thread after
 *   the core lock is released. The callback's diff handle is borrowed for the
 *   callback duration; retain it to keep an additional reference. Read-only
 *   calls are allowed in the callback; project-model, epoch, destruction, and
 *   frame-lease mutations return STITCH_REENTRANT_MUTATION.
 *   A callback must not throw or unwind across this C boundary.
 * - A frame lease is transitioned only on the thread that registered it.
 *   submit rejects a stale revision/epoch. Already-submitted work may still be
 *   completed and retired after cancellation so native resources can drain.
 *   retire invalidates the lease ID and requires all native GPU use to have
 *   completed first.
 * - The bridge owns surface/synchronization tokens and their native objects.
 *   No Rust allocation or platform/Qt object crosses this boundary.
 */
uint32_t stitch_abi_version(void);
stitch_result stitch_abi_negotiate(stitch_abi_negotiation *in_out, stitch_result_detail *out_detail);
stitch_result stitch_core_create(const stitch_core_config *config, stitch_core_handle *out_core, stitch_result_detail *out_detail);
stitch_result stitch_core_destroy(stitch_core_handle core, stitch_result_detail *out_detail);
stitch_result stitch_core_set_epoch(stitch_core_handle core, stitch_cancellation_epoch epoch, stitch_result_detail *out_detail);
stitch_result stitch_core_snapshot(stitch_core_handle core, stitch_snapshot_handle *out_snapshot, stitch_result_detail *out_detail);
stitch_result stitch_core_submit(stitch_core_handle core, const stitch_command_batch *batch, stitch_diff_callback callback, void *context, stitch_diff_handle *out_diff, stitch_result_detail *out_detail);
stitch_result stitch_snapshot_retain(stitch_snapshot_handle snapshot, stitch_result_detail *out_detail);
stitch_result stitch_snapshot_release(stitch_snapshot_handle snapshot, stitch_result_detail *out_detail);
stitch_result stitch_snapshot_metadata_get(stitch_snapshot_handle snapshot, stitch_snapshot_metadata *out_metadata, stitch_result_detail *out_detail);
stitch_result stitch_diff_retain(stitch_diff_handle diff, stitch_result_detail *out_detail);
stitch_result stitch_diff_release(stitch_diff_handle diff, stitch_result_detail *out_detail);
stitch_result stitch_diff_metadata_get(stitch_diff_handle diff, stitch_diff_metadata *out_metadata, stitch_result_detail *out_detail);

/*
 * Frame leases are owned by the bridge thread that registers them. A registered
 * or acquired lease whose wrapper/acquire path fails may be discarded by that
 * same thread. Discard only invalidates the bridge-owned lease ID; it never
 * reports GPU work as completed. Submitted leases must complete and retire.
 */
stitch_result stitch_frame_lease_register(stitch_core_handle core, const stitch_frame_metadata *metadata, stitch_frame_lease_id *out_lease, stitch_result_detail *out_detail);
stitch_result stitch_frame_lease_metadata_get(stitch_core_handle core, stitch_frame_lease_id lease, stitch_frame_metadata *out_metadata, stitch_result_detail *out_detail);
stitch_result stitch_frame_lease_acquire(stitch_core_handle core, stitch_frame_lease_id lease, stitch_device_generation device, stitch_cancellation_epoch epoch, const stitch_digest *revision, stitch_result_detail *out_detail);
stitch_result stitch_frame_lease_discard(stitch_core_handle core, stitch_frame_lease_id lease, stitch_device_generation device, stitch_result_detail *out_detail);
stitch_result stitch_frame_lease_submit(stitch_core_handle core, stitch_frame_lease_id lease, stitch_device_generation device, stitch_result_detail *out_detail);
stitch_result stitch_frame_lease_complete(stitch_core_handle core, stitch_frame_lease_id lease, stitch_device_generation device, stitch_result_detail *out_detail);
stitch_result stitch_frame_lease_retire(stitch_core_handle core, stitch_frame_lease_id lease, stitch_device_generation device, stitch_result_detail *out_detail);

#ifdef __cplusplus
}
#endif
#endif
