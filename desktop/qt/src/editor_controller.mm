#include "editor_controller.h"

#include <QDebug>
#include <QThread>

#include <array>
#include <cstring>
#include <limits>

namespace {

stitch_header header(uint32_t size) {
  return stitch_header{size, STITCH_ABI_VERSION};
}

QString digestPrefix(const stitch_digest &digest) {
  static constexpr char kHex[] = "0123456789abcdef";
  std::array<char, 9> text{};
  for (size_t index = 0; index < 4; ++index) {
    text[index * 2] = kHex[digest.bytes[index] >> 4];
    text[index * 2 + 1] = kHex[digest.bytes[index] & 0x0f];
  }
  return QString::fromLatin1(text.data(), 8);
}

bool sameDigest(const stitch_digest &lhs, const stitch_digest &rhs) {
  return std::memcmp(lhs.bytes, rhs.bytes, STITCH_DIGEST_BYTES) == 0;
}

struct DiffCallbackState {
  Qt::HANDLE expected_thread = nullptr;
  bool called = false;
  bool same_thread = false;
  stitch_diff_handle handle = 0;
  stitch_result metadata_result = STITCH_PANIC;
  stitch_diff_metadata metadata{};
};

void diffCallback(void *opaque, stitch_diff_handle diff) {
  auto *state = static_cast<DiffCallbackState *>(opaque);
  state->called = true;
  state->same_thread = QThread::currentThreadId() == state->expected_thread;
  state->handle = diff;
  stitch_result_detail detail{};
  state->metadata_result =
      stitch_diff_metadata_get(diff, &state->metadata, &detail);
  qInfo().nospace() << "trace abi.diff.callback thread="
                    << QThread::currentThreadId() << " lease=none revision="
                    << digestPrefix(state->metadata.revision);
}

}  // namespace

EditorController::EditorController(QObject *parent) : QObject(parent) {
  stitch_result_detail detail{};
  stitch_abi_negotiation negotiation{
      header(sizeof(stitch_abi_negotiation)), STITCH_ABI_VERSION,
      STITCH_ABI_MINOR_VERSION, 0, 0};
  stitch_result result = stitch_abi_negotiate(&negotiation, &detail);
  if (result != STITCH_OK ||
      negotiation.supported_major != STITCH_ABI_VERSION ||
      negotiation.supported_minor < STITCH_ABI_MINOR_VERSION) {
    (void)fail("stitch_abi_negotiate", result, detail);
    return;
  }

  stitch_core_config config{header(sizeof(stitch_core_config)), 0};
  result = stitch_core_create(&config, &core_, &detail);
  if (result != STITCH_OK || core_ == 0) {
    (void)fail("stitch_core_create", result, detail);
    core_ = 0;
    return;
  }

  status_ = QStringLiteral("ABI %1.%2 ready")
                .arg(negotiation.supported_major)
                .arg(negotiation.supported_minor);
  qInfo().nospace() << "trace abi.core.create thread="
                    << QThread::currentThreadId() << " core=" << core_;
}

EditorController::~EditorController() {
  releaseDiff();
  releaseSnapshot();
  if (core_ != 0) {
    stitch_result_detail detail{};
    const stitch_result result = stitch_core_destroy(core_, &detail);
    if (result != STITCH_OK) {
      qCritical().nospace() << "trace abi.core.destroy.failed result=" << result
                            << " diagnostic=" << detail.diagnostic;
    } else {
      qInfo().nospace() << "trace abi.core.destroy thread="
                        << QThread::currentThreadId() << " core=" << core_;
    }
    core_ = 0;
  }
}

QString EditorController::status() const { return status_; }

bool EditorController::isReady() const { return core_ != 0; }

bool EditorController::frameContext(FrameContext *out_context) const {
  if (out_context == nullptr) return false;
  const std::lock_guard lock(frame_context_mutex_);
  *out_context = frame_context_;
  return out_context->valid;
}

bool EditorController::openSnapshot() { return refreshSnapshot(true); }

bool EditorController::submitDemoBatch() {
  if (core_ == 0 || snapshot_ == 0) {
    status_ = QStringLiteral("open a snapshot before editing");
    emit statusChanged();
    return false;
  }

  stitch_command command{};
  command.header = header(sizeof(stitch_command));
  command.kind = STITCH_COMMAND_ADD_TRACK;
  const uint64_t track_id = next_track_id_++;
  for (size_t byte = 0; byte < sizeof(track_id); ++byte) {
    command.primary_id.bytes[byte] =
        static_cast<uint8_t>(track_id >> (byte * 8));
  }
  command.primary_id.bytes[STITCH_ID_BYTES - 1] = 0x54;

  stitch_command_batch batch{};
  batch.header = header(sizeof(stitch_command_batch));
  batch.commands = &command;
  batch.command_count = 1;
  batch.expected_revision = snapshot_metadata_.revision;
  batch.epoch = snapshot_metadata_.epoch;

  DiffCallbackState callback_state;
  callback_state.expected_thread = QThread::currentThreadId();
  stitch_result_detail detail{};
  stitch_diff_handle next_diff = 0;
  const stitch_result result = stitch_core_submit(
      core_, &batch, diffCallback, &callback_state, &next_diff, &detail);
  if (result != STITCH_OK) return fail("stitch_core_submit", result, detail);
  if (!callback_state.called || !callback_state.same_thread ||
      callback_state.metadata_result != STITCH_OK ||
      callback_state.handle != next_diff || next_diff == 0) {
    if (next_diff != 0) stitch_diff_release(next_diff, &detail);
    status_ = QStringLiteral("synchronous ABI diff callback contract failed");
    qCritical() << "trace abi.diff.callback.contract.failed";
    emit statusChanged();
    return false;
  }

  releaseDiff();
  diff_ = next_diff;
  if (!refreshSnapshot(false)) return false;
  if (!sameDigest(snapshot_metadata_.revision,
                  callback_state.metadata.revision) ||
      !sameDigest(snapshot_metadata_.project_digest,
                  callback_state.metadata.project_digest)) {
    status_ = QStringLiteral("snapshot and diff metadata diverged");
    qCritical() << "trace abi.snapshot.diff.diverged";
    emit statusChanged();
    return false;
  }

  status_ = QStringLiteral("track %1 committed at %2")
                .arg(track_id)
                .arg(digestPrefix(snapshot_metadata_.revision));
  emit statusChanged();
  qInfo().nospace() << "trace abi.batch.submit thread="
                    << QThread::currentThreadId() << " epoch="
                    << snapshot_metadata_.epoch << " revision="
                    << digestPrefix(snapshot_metadata_.revision);
  return true;
}

bool EditorController::cancelEpoch() {
  if (core_ == 0) return false;
  FrameContext current;
  if (!frameContext(&current) ||
      current.epoch == std::numeric_limits<uint64_t>::max()) {
    status_ = QStringLiteral("cannot advance cancellation epoch");
    emit statusChanged();
    return false;
  }
  const stitch_cancellation_epoch next = current.epoch + 1;
  stitch_result_detail detail{};
  const stitch_result result = stitch_core_set_epoch(core_, next, &detail);
  if (result != STITCH_OK) return fail("stitch_core_set_epoch", result, detail);
  if (!refreshSnapshot(false)) return false;
  status_ = QStringLiteral("preview epoch %1 cancelled").arg(next);
  emit statusChanged();
  qInfo().nospace() << "trace abi.epoch.cancel thread="
                    << QThread::currentThreadId() << " epoch=" << next
                    << " revision=" << digestPrefix(snapshot_metadata_.revision);
  return true;
}

bool EditorController::registerAcquireFrame(
    stitch_device_generation device_generation, uint64_t surface_token,
    uint64_t synchronization_token, uint32_t width, uint32_t height,
    stitch_frame_lease_id *out_lease, FrameContext *out_context) {
  if (out_lease == nullptr || out_context == nullptr || core_ == 0) return false;
  FrameContext context;
  if (!frameContext(&context)) return false;

  stitch_frame_metadata metadata{};
  metadata.header = header(sizeof(stitch_frame_metadata));
  metadata.revision = context.revision;
  metadata.epoch = context.epoch;
  metadata.device_generation = device_generation;
  metadata.surface_token = surface_token;
  metadata.synchronization_token = synchronization_token;
  metadata.pixel_width = width;
  metadata.pixel_height = height;
  metadata.pixel_format = STITCH_PIXEL_FORMAT_RGBA8_UNORM;
  metadata.synchronization_kind = STITCH_SYNCHRONIZATION_BRIDGE_TOKEN;
  metadata.synchronization_value = 1;
  metadata.presentation_time = stitch_rational{0, 1};
  metadata.state = STITCH_LEASE_REGISTERED;

  stitch_result_detail detail{};
  stitch_frame_lease_id lease = 0;
  stitch_result result = stitch_frame_lease_register(
      core_, &metadata, &lease, &detail);
  if (result != STITCH_OK) return fail("stitch_frame_lease_register", result, detail);
  result = stitch_frame_lease_acquire(core_, lease, device_generation,
                                      context.epoch, &context.revision, &detail);
  if (result != STITCH_OK) {
    const stitch_result_detail acquire_detail = detail;
    (void)discardFrame(device_generation, lease);
    return fail("stitch_frame_lease_acquire", result, acquire_detail);
  }
  stitch_frame_metadata confirmed{};
  result = stitch_frame_lease_metadata_get(core_, lease, &confirmed, &detail);
  if (result != STITCH_OK) {
    const stitch_result_detail metadata_detail = detail;
    (void)discardFrame(device_generation, lease);
    return fail("stitch_frame_lease_metadata_get", result, metadata_detail);
  }
  if (confirmed.state != STITCH_LEASE_ACQUIRED ||
      confirmed.owner_thread_token == 0 ||
      confirmed.surface_token != surface_token ||
      confirmed.synchronization_token != synchronization_token) {
    (void)discardFrame(device_generation, lease);
    stitch_result_detail contract_detail{
        header(sizeof(stitch_result_detail)), STITCH_BAD_LEASE_STATE, 1, 0};
    return fail("stitch_frame_lease_metadata_contract",
                STITCH_BAD_LEASE_STATE, contract_detail);
  }

  *out_lease = lease;
  *out_context = context;
  qInfo().nospace() << "trace lease.acquire thread="
                    << QThread::currentThreadId() << " device="
                    << device_generation << " epoch=" << context.epoch
                    << " revision=" << digestPrefix(context.revision)
                    << " lease=" << lease << " surface=" << surface_token;
  return true;
}

bool EditorController::submitFrame(
    stitch_device_generation device_generation,
    stitch_frame_lease_id lease) {
  if (core_ == 0 || lease == 0) return false;
  stitch_result_detail detail{};
  const stitch_result result =
      stitch_frame_lease_submit(core_, lease, device_generation, &detail);
  if (result != STITCH_OK) {
    const stitch_result_detail submit_detail = detail;
    (void)discardFrame(device_generation, lease);
    return fail("stitch_frame_lease_submit", result, submit_detail);
  }
  qInfo().nospace() << "trace lease.submit thread="
                    << QThread::currentThreadId() << " device="
                    << device_generation << " lease=" << lease;
  return true;
}

bool EditorController::discardFrame(
    stitch_device_generation device_generation,
    stitch_frame_lease_id lease) {
  if (core_ == 0 || lease == 0) return false;
  stitch_result_detail detail{};
  const stitch_result result =
      stitch_frame_lease_discard(core_, lease, device_generation, &detail);
  if (result != STITCH_OK) return fail("stitch_frame_lease_discard", result, detail);
  qInfo().nospace() << "trace lease.discard thread="
                    << QThread::currentThreadId() << " device="
                    << device_generation << " lease=" << lease;
  return true;
}

bool EditorController::completeAndRetireFrame(
    stitch_device_generation device_generation,
    stitch_frame_lease_id lease) {
  if (core_ == 0 || lease == 0) return false;
  stitch_result_detail detail{};
  stitch_result result =
      stitch_frame_lease_complete(core_, lease, device_generation, &detail);
  if (result != STITCH_OK) return fail("stitch_frame_lease_complete", result, detail);
  qInfo().nospace() << "trace lease.complete thread="
                    << QThread::currentThreadId() << " device="
                    << device_generation << " lease=" << lease;
  result = stitch_frame_lease_retire(core_, lease, device_generation, &detail);
  if (result != STITCH_OK) return fail("stitch_frame_lease_retire", result, detail);
  qInfo().nospace() << "trace lease.retire thread="
                    << QThread::currentThreadId() << " device="
                    << device_generation << " lease=" << lease;
  return true;
}

bool EditorController::refreshSnapshot(bool publish_status) {
  if (core_ == 0) return false;
  stitch_result_detail detail{};
  stitch_snapshot_handle next_snapshot = 0;
  stitch_result result =
      stitch_core_snapshot(core_, &next_snapshot, &detail);
  if (result != STITCH_OK) return fail("stitch_core_snapshot", result, detail);

  stitch_snapshot_metadata next_metadata{};
  result = stitch_snapshot_metadata_get(next_snapshot, &next_metadata, &detail);
  if (result != STITCH_OK) {
    stitch_snapshot_release(next_snapshot, &detail);
    return fail("stitch_snapshot_metadata_get", result, detail);
  }

  releaseSnapshot();
  snapshot_ = next_snapshot;
  snapshot_metadata_ = next_metadata;
  publishFrameContext(next_metadata);
  if (publish_status) {
    status_ = QStringLiteral("snapshot %1 opened")
                  .arg(digestPrefix(next_metadata.revision));
    emit statusChanged();
  }
  qInfo().nospace() << "trace abi.snapshot.open thread="
                    << QThread::currentThreadId() << " epoch="
                    << next_metadata.epoch << " revision="
                    << digestPrefix(next_metadata.revision) << " tracks="
                    << next_metadata.track_count;
  return true;
}

bool EditorController::fail(const char *operation, stitch_result result,
                            const stitch_result_detail &detail) {
  qCritical().nospace() << "trace abi.failure operation=" << operation
                        << " result=" << result
                        << " domain=" << detail.domain
                        << " diagnostic=" << detail.diagnostic
                        << " thread=" << QThread::currentThreadId();
  if (QThread::currentThread() == thread()) {
    status_ = QStringLiteral("%1 failed (%2)")
                  .arg(QString::fromLatin1(operation))
                  .arg(result);
    emit statusChanged();
  }
  return false;
}

void EditorController::publishFrameContext(
    const stitch_snapshot_metadata &metadata) {
  {
    const std::lock_guard lock(frame_context_mutex_);
    frame_context_.revision = metadata.revision;
    frame_context_.epoch = metadata.epoch;
    frame_context_.valid = true;
  }
  emit frameContextChanged();
}

void EditorController::releaseSnapshot() {
  if (snapshot_ == 0) return;
  stitch_result_detail detail{};
  const stitch_result result = stitch_snapshot_release(snapshot_, &detail);
  if (result != STITCH_OK) {
    qCritical().nospace() << "trace abi.snapshot.release.failed handle="
                          << snapshot_ << " result=" << result;
  }
  snapshot_ = 0;
}

void EditorController::releaseDiff() {
  if (diff_ == 0) return;
  stitch_result_detail detail{};
  const stitch_result result = stitch_diff_release(diff_, &detail);
  if (result != STITCH_OK) {
    qCritical().nospace() << "trace abi.diff.release.failed handle=" << diff_
                          << " result=" << result;
  }
  diff_ = 0;
}
