#pragma once

#include <QMetaObject>
#include <QQuickItem>
#include <QTimer>

#include <atomic>
#include <memory>

#include "editor_controller.h"

class CompletionMailbox;
class PreviewNode;

class PreviewItem : public QQuickItem {
  Q_OBJECT

 public:
  explicit PreviewItem(QQuickItem *parent = nullptr);
  ~PreviewItem() override;

  void setController(EditorController *controller);
  void setSelfTestMode(bool enabled);
  void setExpectedGraphicsDevice(const void *device);

 signals:
  void leaseSubmitted(qulonglong lease, qulonglong epoch);
  void leaseRetired(qulonglong lease);
  void lifecycleFailed(const QString &message);
  void sceneGraphTeardownObserved();

 protected:
  QSGNode *updatePaintNode(QSGNode *old_node,
                           UpdatePaintNodeData *data) override;
  void geometryChange(const QRectF &new_geometry,
                      const QRectF &old_geometry) override;

 private:
  void attachWindowSignals();
  void onAfterRendering();
  void queueFailure(const QString &message);

  EditorController *controller_ = nullptr;
  QTimer pump_timer_;
  std::shared_ptr<CompletionMailbox> mailbox_;
  QMetaObject::Connection after_rendering_connection_;
  QMetaObject::Connection invalidated_connection_;
  PreviewNode *render_node_ = nullptr;  // Render-thread access only.
  EditorController::FrameContext last_context_{};
  stitch_device_generation device_generation_ = 1;
  std::atomic_bool pump_active_{false};
  std::atomic_bool self_test_mode_{false};
  bool self_test_generation_done_ = false;  // Render-thread access only.
  const void *expected_graphics_device_ = nullptr;
};
