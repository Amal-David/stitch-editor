#include <cstddef>
#include <type_traits>

#include <stitch/c_api.h>

static_assert(std::is_standard_layout_v<stitch_command>);
static_assert(std::is_trivially_copyable_v<stitch_command>);
static_assert(std::is_trivially_copyable_v<stitch_frame_metadata>);
static_assert(sizeof(stitch_digest) == STITCH_DIGEST_BYTES);
static_assert(sizeof(stitch_id) == STITCH_ID_BYTES);
static_assert(sizeof(stitch_result) == sizeof(uint32_t));
static_assert(sizeof(void*) == 8);
static_assert(sizeof(stitch_header) == 8 && alignof(stitch_header) == 4);
static_assert(sizeof(stitch_rational) == 16 && alignof(stitch_rational) == 8);
static_assert(sizeof(stitch_time_range) == 32 && alignof(stitch_time_range) == 8);
static_assert(sizeof(stitch_result_detail) == 24 && alignof(stitch_result_detail) == 8);
static_assert(sizeof(stitch_abi_negotiation) == 24 && alignof(stitch_abi_negotiation) == 4);
static_assert(sizeof(stitch_core_config) == 16 && alignof(stitch_core_config) == 8);
static_assert(sizeof(stitch_command) == 184 && alignof(stitch_command) == 8);
static_assert(sizeof(stitch_command_batch) == 64 && alignof(stitch_command_batch) == 8);
static_assert(sizeof(stitch_snapshot_metadata) == 104 && alignof(stitch_snapshot_metadata) == 8);
static_assert(sizeof(stitch_diff_metadata) == 112 && alignof(stitch_diff_metadata) == 8);
static_assert(sizeof(stitch_frame_metadata) == 136 && alignof(stitch_frame_metadata) == 8);
static_assert(offsetof(stitch_command, primary_id) == 16);
static_assert(offsetof(stitch_command, content_sha256) == 64);
static_assert(offsetof(stitch_command, timeline_range) == 120);
static_assert(offsetof(stitch_command_batch, commands) == 8);
static_assert(offsetof(stitch_command_batch, expected_revision) == 24);
static_assert(offsetof(stitch_frame_metadata, revision) == 16);
static_assert(offsetof(stitch_frame_metadata, surface_token) == 64);
static_assert(offsetof(stitch_frame_metadata, pixel_width) == 88);
static_assert(offsetof(stitch_frame_metadata, synchronization_value) == 104);
static_assert(offsetof(stitch_frame_metadata, presentation_time) == 112);
static_assert(offsetof(stitch_frame_metadata, state) == 128);

static stitch_core_handle callback_core = 0;
static stitch_result callback_snapshot_result = STITCH_OK;
static stitch_result callback_metadata_result = STITCH_OK;
static stitch_result callback_set_epoch_result = STITCH_OK;
static stitch_result callback_destroy_result = STITCH_OK;
static void reentrant_callback(void*, stitch_diff_handle) {
  stitch_result_detail detail{};
  stitch_snapshot_handle snapshot = 0;
  callback_snapshot_result = stitch_core_snapshot(callback_core, &snapshot, &detail);
  if (callback_snapshot_result == STITCH_OK) {
    stitch_snapshot_metadata metadata{};
    callback_metadata_result = stitch_snapshot_metadata_get(snapshot, &metadata, &detail);
    (void)stitch_snapshot_release(snapshot, &detail);
  }
  callback_set_epoch_result = stitch_core_set_epoch(callback_core, 1, &detail);
  callback_destroy_result = stitch_core_destroy(callback_core, &detail);
}

int main() {
  stitch_result_detail detail{};
  stitch_abi_negotiation negotiation{{sizeof(stitch_abi_negotiation), STITCH_ABI_VERSION},
                                    STITCH_ABI_VERSION, 0, 0, 0};
  if (stitch_abi_version() != STITCH_ABI_VERSION ||
      stitch_abi_negotiate(&negotiation, &detail) != STITCH_OK ||
      negotiation.supported_major != STITCH_ABI_VERSION || negotiation.supported_minor != 0) return 1;
  stitch_core_config config{{sizeof(stitch_core_config), STITCH_ABI_VERSION}, 0};
  stitch_core_handle core = 0;
  if (stitch_core_create(&config, &core, &detail) != STITCH_OK || core == 0) return 2;
  stitch_command command{};
  command.header = {sizeof(command), STITCH_ABI_VERSION};
  command.kind = STITCH_COMMAND_ADD_TRACK;
  command.primary_id.bytes[0] = 1;
  stitch_command_batch batch{{sizeof(batch), STITCH_ABI_VERSION}, &command, 1, 0, {}, 0};
  stitch_diff_handle diff = 0;
  callback_core = core;
  command.content_sha256.bytes[0] = 1;
  if (stitch_core_submit(core, &batch, nullptr, nullptr, &diff, &detail) !=
      STITCH_INVALID_ARGUMENT) return 3;
  command.content_sha256.bytes[0] = 0;
  batch.reserved = 1;
  if (stitch_core_submit(core, &batch, nullptr, nullptr, &diff, &detail) !=
      STITCH_INVALID_BATCH) return 3;
  batch.reserved = 0;
  if (stitch_core_submit(core, &batch, reentrant_callback, nullptr, &diff, &detail) != STITCH_OK || diff == 0) return 3;
  if (callback_snapshot_result != STITCH_OK || callback_metadata_result != STITCH_OK ||
      callback_set_epoch_result != STITCH_REENTRANT_MUTATION ||
      callback_destroy_result != STITCH_REENTRANT_MUTATION) return 4;

  stitch_snapshot_handle snapshot = 0;
  stitch_snapshot_metadata snapshot_metadata{};
  if (stitch_core_snapshot(core, &snapshot, &detail) != STITCH_OK ||
      stitch_snapshot_metadata_get(snapshot, &snapshot_metadata, &detail) != STITCH_OK ||
      stitch_snapshot_release(snapshot, &detail) != STITCH_OK) return 5;

  stitch_diff_metadata diff_metadata{};
  if (stitch_diff_metadata_get(diff, &diff_metadata, &detail) != STITCH_OK) return 6;
  stitch_frame_metadata frame{};
  frame.header = {sizeof(frame), STITCH_ABI_VERSION};
  frame.revision = diff_metadata.revision;
  frame.epoch = 0;
  frame.device_generation = 7;
  frame.surface_token = 9;
  frame.synchronization_token = 10;
  frame.pixel_width = 16;
  frame.pixel_height = 16;
  frame.pixel_format = STITCH_PIXEL_FORMAT_RGBA8_UNORM;
  frame.synchronization_kind = STITCH_SYNCHRONIZATION_BRIDGE_TOKEN;
  frame.synchronization_value = 1;
  frame.presentation_time = {0, 1};
  frame.state = STITCH_LEASE_REGISTERED;
  stitch_frame_lease_id lease = 0;
  if (stitch_frame_lease_register(core, &frame, &lease, &detail) != STITCH_OK || lease == 0) return 7;
  if (stitch_frame_lease_acquire(core, lease, 7, 0, &diff_metadata.revision, &detail) != STITCH_OK ||
      stitch_frame_lease_submit(core, lease, 7, &detail) != STITCH_OK ||
      stitch_frame_lease_complete(core, lease, 7, &detail) != STITCH_OK ||
      stitch_frame_lease_retire(core, lease, 7, &detail) != STITCH_OK) return 8;
  stitch_frame_metadata retired{};
  if (stitch_frame_lease_metadata_get(core, lease, &retired, &detail) != STITCH_UNKNOWN_HANDLE) return 9;

  stitch_frame_lease_id discarded = 0;
  if (stitch_frame_lease_register(core, &frame, &discarded, &detail) != STITCH_OK || discarded == 0 ||
      stitch_frame_lease_acquire(core, discarded, 7, 0, &diff_metadata.revision, &detail) != STITCH_OK ||
      stitch_frame_lease_discard(core, discarded, 7, &detail) != STITCH_OK) return 10;
  if (stitch_frame_lease_metadata_get(core, discarded, &retired, &detail) != STITCH_UNKNOWN_HANDLE) return 11;

  if (stitch_diff_release(diff, &detail) != STITCH_OK) return 12;
  return stitch_core_destroy(core, &detail) == STITCH_OK ? 0 : 13;
}
