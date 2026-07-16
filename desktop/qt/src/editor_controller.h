#pragma once

#include <QObject>
#include <QString>

#include <cstdint>
#include <mutex>

#include <stitch/c_api.h>

class EditorController final : public QObject {
  Q_OBJECT
  Q_PROPERTY(QString status READ status NOTIFY statusChanged)

 public:
  struct FrameContext {
    stitch_digest revision{};
    stitch_cancellation_epoch epoch = 0;
    bool valid = false;
  };

  explicit EditorController(QObject *parent = nullptr);
  ~EditorController() override;

  EditorController(const EditorController &) = delete;
  EditorController &operator=(const EditorController &) = delete;

  [[nodiscard]] QString status() const;
  [[nodiscard]] bool isReady() const;
  [[nodiscard]] bool frameContext(FrameContext *out_context) const;

  Q_INVOKABLE bool openSnapshot();
  Q_INVOKABLE bool submitDemoBatch();
  Q_INVOKABLE bool cancelEpoch();

  // Render-thread bridge entrypoints. Native objects never cross the C ABI.
  [[nodiscard]] bool registerAcquireFrame(
      stitch_device_generation device_generation, uint64_t surface_token,
      uint64_t synchronization_token, uint32_t width, uint32_t height,
      stitch_frame_lease_id *out_lease, FrameContext *out_context);
  [[nodiscard]] bool submitFrame(
      stitch_device_generation device_generation,
      stitch_frame_lease_id lease);
  [[nodiscard]] bool discardFrame(
      stitch_device_generation device_generation,
      stitch_frame_lease_id lease);
  [[nodiscard]] bool completeAndRetireFrame(
      stitch_device_generation device_generation,
      stitch_frame_lease_id lease);

 signals:
  void statusChanged();
  void frameContextChanged();

 private:
  [[nodiscard]] bool refreshSnapshot(bool publish_status);
  [[nodiscard]] bool fail(const char *operation, stitch_result result,
                          const stitch_result_detail &detail);
  void publishFrameContext(const stitch_snapshot_metadata &metadata);
  void releaseSnapshot();
  void releaseDiff();

  stitch_core_handle core_ = 0;
  stitch_snapshot_handle snapshot_ = 0;
  stitch_diff_handle diff_ = 0;
  stitch_snapshot_metadata snapshot_metadata_{};
  mutable std::mutex frame_context_mutex_;
  FrameContext frame_context_{};
  QString status_ = QStringLiteral("initializing");
  uint64_t next_track_id_ = 1;
};
