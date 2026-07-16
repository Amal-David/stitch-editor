#include "preview_item.h"

#include <QDebug>
#include <QQuickWindow>
#include <QSGRendererInterface>
#include <QSGSimpleTextureNode>
#include <QThread>

#include <QtQuick/qsgtexture_platform.h>

#include <atomic>
#include <cstring>

#if defined(STITCH_EXPECT_METAL)
#import <Metal/Metal.h>
#endif

namespace {

std::atomic<uint64_t> next_bridge_token{1};

bool sameDigest(const stitch_digest &lhs, const stitch_digest &rhs) {
  return std::memcmp(lhs.bytes, rhs.bytes, STITCH_DIGEST_BYTES) == 0;
}

}  // namespace

// One preview generation is in flight, so completion publication is a bounded,
// allocation-free atomic handoff from Metal to the Qt render thread.
class CompletionMailbox final {
 public:
  void enqueue(stitch_frame_lease_id lease) {
    stitch_frame_lease_id empty = 0;
    (void)completed_.compare_exchange_strong(empty, lease,
                                              std::memory_order_release,
                                              std::memory_order_relaxed);
  }

  bool take(stitch_frame_lease_id lease) {
    return completed_.compare_exchange_strong(
        lease, 0, std::memory_order_acq_rel, std::memory_order_acquire);
  }

  void discard(stitch_frame_lease_id lease) {
    (void)completed_.compare_exchange_strong(
        lease, 0, std::memory_order_acq_rel, std::memory_order_acquire);
  }

 private:
  std::atomic<stitch_frame_lease_id> completed_{0};
};

// Stateful parent node: the visual child is detached on the first sync pass
// after its command-buffer completion handler is installed. The external
// wrapper and native texture stay retained until both detachment and actual GPU
// completion are proven on a later render-thread pass.
class PreviewNode final : public QSGNode {
 public:
#if defined(STITCH_EXPECT_METAL)
  PreviewNode(EditorController *controller,
              std::shared_ptr<CompletionMailbox> mailbox,
              QQuickWindow *window, id<MTLDevice> device,
              stitch_device_generation device_generation)
      : controller_(controller),
        mailbox_(std::move(mailbox)),
        device_generation_(device_generation) {
    MTLTextureDescriptor *descriptor =
        [MTLTextureDescriptor
            texture2DDescriptorWithPixelFormat:MTLPixelFormatRGBA8Unorm
                                         width:2
                                        height:2
                                     mipmapped:NO];
    descriptor.usage = MTLTextureUsageShaderRead;
    descriptor.storageMode = MTLStorageModeShared;
    texture_ = [device newTextureWithDescriptor:descriptor];
    if (texture_ == nil || texture_.device != device) {
      fail(QStringLiteral("same-device Metal texture creation failed"));
      return;
    }
    texture_.label = @"Stitch synthetic RGBA preview";

    static constexpr uint8_t kPixels[] = {
        255, 70, 70, 255, 70, 255, 120, 255,
        70, 120, 255, 255, 255, 220, 70, 255};
    [texture_ replaceRegion:MTLRegionMake2D(0, 0, 2, 2)
                mipmapLevel:0
                  withBytes:kPixels
                bytesPerRow:8];

    texture_wrapper_ = QNativeInterface::QSGMetalTexture::fromNative(
        texture_, window, QSize(2, 2), QQuickWindow::TextureHasAlphaChannel);
    if (texture_wrapper_ == nullptr) {
      fail(QStringLiteral("Qt rejected the external Metal texture"));
      releaseNativeObjects();
      return;
    }
    auto *native_texture = texture_wrapper_
                               ->nativeInterface<
                                   QNativeInterface::QSGMetalTexture>();
    if (native_texture == nullptr ||
        native_texture->nativeTexture() != texture_) {
      fail(QStringLiteral("Qt external texture identity mismatch"));
      releaseNativeObjects();
      return;
    }

    visual_ = new QSGSimpleTextureNode;
    visual_->setOwnsTexture(false);
    visual_->setTexture(texture_wrapper_);
    visual_->setFiltering(QSGTexture::Linear);
    appendChildNode(visual_);

    const uint64_t surface_token = next_bridge_token.fetch_add(1);
    const uint64_t synchronization_token = next_bridge_token.fetch_add(1);
    if (!controller_->registerAcquireFrame(
            device_generation_, surface_token, synchronization_token, 2, 2,
            &lease_, &frame_context_)) {
      fail(QStringLiteral("frame lease register/acquire failed"));
      releaseNativeObjects();
      return;
    }
    acquired_ = true;

    // Submit before returning a drawable node. A failed ABI submit therefore
    // cannot strand a draw whose command buffer has not yet been observed.
    if (!controller_->submitFrame(device_generation_, lease_)) {
      // submitFrame discards the still-Acquired lease on failure.
      acquired_ = false;
      lease_ = 0;
      fail(QStringLiteral("frame lease submit failed"));
      releaseNativeObjects();
      return;
    }
    acquired_ = false;
    submitted_ = true;
    valid_ = true;
  }
#endif

  ~PreviewNode() override {
#if defined(STITCH_EXPECT_METAL)
    detachVisual();
    if (submitted_ && !retired_ && lease_ != 0) {
      if (command_buffer_ != nil) {
        // Teardown-only fallback. Normal operation retires through the mailbox
        // on a later render pass; teardown waits only when that pass is gone.
        [command_buffer_ waitUntilCompleted];
      }
      // With no command buffer, scene-graph teardown means this generation was
      // never submitted by Qt or Qt has already made its resources idle.
      if (controller_->completeAndRetireFrame(device_generation_, lease_)) {
        retired_ = true;
        qInfo().nospace()
            << "trace lease.retire.teardown-fallback thread="
            << QThread::currentThreadId() << " device=" << device_generation_
            << " lease=" << lease_;
      }
    } else if (acquired_ && lease_ != 0) {
      (void)controller_->discardFrame(device_generation_, lease_);
      acquired_ = false;
    }
    mailbox_->discard(lease_);
    releaseNativeObjects();
#endif
  }

  PreviewNode(const PreviewNode &) = delete;
  PreviewNode &operator=(const PreviewNode &) = delete;

  [[nodiscard]] bool isValid() const { return valid_; }
  [[nodiscard]] bool hasFailed() const { return failed_; }
  [[nodiscard]] bool completionAttached() const {
    return completion_attached_;
  }
  [[nodiscard]] bool detachRequested() const { return detach_requested_; }
  [[nodiscard]] stitch_frame_lease_id lease() const { return lease_; }
  [[nodiscard]] const EditorController::FrameContext &frameContext() const {
    return frame_context_;
  }
  [[nodiscard]] QString failureMessage() const { return failure_message_; }

  void setRect(const QRectF &rect) {
    if (visual_ != nullptr) visual_->setRect(rect);
  }

  void detachVisual() {
    if (visual_ == nullptr) return;
    removeChildNode(visual_);
    // ownsTexture is false, so deleting the visual node detaches its material
    // without dereferencing or destroying the retained external wrapper.
    delete visual_;
    visual_ = nullptr;
    detached_ = true;
    qInfo().nospace() << "trace preview.texture.detach thread="
                      << QThread::currentThreadId() << " device="
                      << device_generation_ << " epoch="
                      << frame_context_.epoch << " lease=" << lease_;
  }

#if defined(STITCH_EXPECT_METAL)
  bool attachCompletion(id<MTLCommandBuffer> command_buffer) {
    if (!valid_ || failed_ || completion_attached_ || command_buffer == nil)
      return false;
    if (command_buffer.commandQueue.device != texture_.device) {
      fail(QStringLiteral("Metal command buffer device mismatch"));
      return false;
    }

    command_buffer_ = [command_buffer retain];
    const std::shared_ptr<CompletionMailbox> mailbox = mailbox_;
    const stitch_frame_lease_id submitted_lease = lease_;
    [command_buffer_ addCompletedHandler:^(id<MTLCommandBuffer>) {
      // The Metal callback never calls Qt or the core. It only publishes an
      // opaque lease ID for the next render-thread drain.
      mailbox->enqueue(submitted_lease);
    }];
    completion_attached_ = true;
    detach_requested_ = true;
    return true;
  }
#endif

  bool drainCompletedLease() {
    if (!submitted_ || !detached_ || retired_ ||
        !mailbox_->take(lease_))
      return false;
    qInfo().nospace() << "trace gpu.completion.dequeue thread="
                      << QThread::currentThreadId() << " device="
                      << device_generation_ << " epoch="
                      << frame_context_.epoch << " lease=" << lease_;
    if (!controller_->completeAndRetireFrame(device_generation_, lease_)) {
      fail(QStringLiteral("frame lease completion/retirement failed"));
      return false;
    }
    retired_ = true;
    releaseNativeObjects();
    return true;
  }

  void failAfterSubmit(const QString &reason) {
    detach_requested_ = true;
    fail(reason);
  }

 private:
  void fail(const QString &message) {
    valid_ = false;
    failed_ = true;
    failure_message_ = message;
    qCritical().nospace() << "trace preview.failure thread="
                          << QThread::currentThreadId() << " message="
                          << message;
  }

  void releaseNativeObjects() {
#if defined(STITCH_EXPECT_METAL)
    detachVisual();
    if (texture_wrapper_ != nullptr) {
      delete texture_wrapper_;
      texture_wrapper_ = nullptr;
    }
    if (texture_ != nil) {
      [texture_ release];
      texture_ = nil;
    }
    if (command_buffer_ != nil) {
      [command_buffer_ release];
      command_buffer_ = nil;
    }
#endif
  }

  EditorController *controller_ = nullptr;
  std::shared_ptr<CompletionMailbox> mailbox_;
  QSGSimpleTextureNode *visual_ = nullptr;
  stitch_device_generation device_generation_ = 0;
  stitch_frame_lease_id lease_ = 0;
  EditorController::FrameContext frame_context_{};
  bool valid_ = false;
  bool acquired_ = false;
  bool submitted_ = false;
  bool completion_attached_ = false;
  bool detach_requested_ = false;
  bool detached_ = false;
  bool retired_ = false;
  bool failed_ = false;
  QString failure_message_;
#if defined(STITCH_EXPECT_METAL)
  id<MTLTexture> texture_ = nil;
  id<MTLCommandBuffer> command_buffer_ = nil;
  QSGTexture *texture_wrapper_ = nullptr;
#endif
};

PreviewItem::PreviewItem(QQuickItem *parent)
    : QQuickItem(parent), mailbox_(std::make_shared<CompletionMailbox>()) {
  setFlag(ItemHasContents, true);
  setActiveFocusOnTab(true);
  pump_timer_.setInterval(8);
  pump_timer_.setTimerType(Qt::PreciseTimer);
  connect(&pump_timer_, &QTimer::timeout, this, [this] {
    if (!pump_active_.load()) {
      pump_timer_.stop();
      return;
    }
    update();
  });
}

PreviewItem::~PreviewItem() {
  pump_active_.store(false);
  pump_timer_.stop();
  disconnect(after_rendering_connection_);
  disconnect(invalidated_connection_);
}

void PreviewItem::setController(EditorController *controller) {
  Q_ASSERT(QThread::currentThread() == thread());
  if (controller_ == controller) return;
  if (controller_ != nullptr) disconnect(controller_, nullptr, this, nullptr);
  controller_ = controller;
  if (controller_ != nullptr) {
    connect(controller_, &EditorController::frameContextChanged, this, [this] {
      pump_active_.store(true);
      if (!pump_timer_.isActive()) pump_timer_.start();
      update();
    });
  }
  attachWindowSignals();
  pump_active_.store(true);
  if (!pump_timer_.isActive()) pump_timer_.start();
  update();
}

void PreviewItem::setSelfTestMode(bool enabled) {
  self_test_mode_.store(enabled);
}

void PreviewItem::setExpectedGraphicsDevice(const void *device) {
  Q_ASSERT(QThread::currentThread() == thread());
  expected_graphics_device_ = device;
}

QSGNode *PreviewItem::updatePaintNode(QSGNode *old_node,
                                      UpdatePaintNodeData *) {
  auto *node = static_cast<PreviewNode *>(old_node);
  render_node_ = node;

  if (node != nullptr && node->detachRequested()) node->detachVisual();
  if (node != nullptr && node->drainCompletedLease()) {
    const stitch_frame_lease_id retired_lease = node->lease();
    last_context_ = node->frameContext();
    delete node;
    render_node_ = nullptr;
    self_test_generation_done_ = self_test_mode_.load();
    pump_active_.store(false);
    QMetaObject::invokeMethod(
        this,
        [this, retired_lease] { emit leaseRetired(retired_lease); },
        Qt::QueuedConnection);
    return nullptr;
  }

  if (node != nullptr && node->hasFailed()) {
    const QString message = node->failureMessage();
    delete node;
    render_node_ = nullptr;
    pump_active_.store(false);
    queueFailure(message);
    return nullptr;
  }

  if (node != nullptr) {
    node->setRect(boundingRect());
    return node;
  }
  if (controller_ == nullptr || self_test_generation_done_) return nullptr;

  EditorController::FrameContext context;
  if (!controller_->frameContext(&context)) return nullptr;
  if (last_context_.valid && context.epoch == last_context_.epoch &&
      sameDigest(context.revision, last_context_.revision)) {
    pump_active_.store(false);
    return nullptr;
  }

#if defined(STITCH_EXPECT_METAL)
  QQuickWindow *quick_window = window();
  if (quick_window == nullptr ||
      quick_window->rendererInterface()->graphicsApi() !=
          QSGRendererInterface::Metal) {
    pump_active_.store(false);
    queueFailure(QStringLiteral("Metal scene graph is unavailable"));
    return nullptr;
  }
  void *device_resource = quick_window->rendererInterface()->getResource(
      quick_window, QSGRendererInterface::DeviceResource);
  if (device_resource == nullptr ||
      device_resource != expected_graphics_device_) {
    pump_active_.store(false);
    queueFailure(QStringLiteral("Qt Metal DeviceResource identity mismatch"));
    return nullptr;
  }
  id<MTLDevice> device = (id<MTLDevice>)device_resource;
  node = new PreviewNode(controller_, mailbox_, quick_window, device,
                         device_generation_);
#else
  queueFailure(QStringLiteral("native preview bridge is not implemented"));
  return nullptr;
#endif
  if (!node->isValid()) {
    const QString message = node->failureMessage();
    delete node;
    pump_active_.store(false);
    queueFailure(message);
    return nullptr;
  }
  node->setRect(boundingRect());
  render_node_ = node;
  pump_active_.store(true);
  const stitch_frame_lease_id submitted_lease = node->lease();
  const stitch_cancellation_epoch submitted_epoch = node->frameContext().epoch;
  qInfo().nospace() << "trace preview.texture.wrap thread="
                    << QThread::currentThreadId() << " device="
                    << device_generation_ << " epoch=" << submitted_epoch
                    << " lease=" << submitted_lease;
  QMetaObject::invokeMethod(
      this,
      [this, submitted_lease, submitted_epoch] {
        emit leaseSubmitted(submitted_lease, submitted_epoch);
      },
      Qt::QueuedConnection);
  return node;
}

void PreviewItem::geometryChange(const QRectF &new_geometry,
                                 const QRectF &old_geometry) {
  QQuickItem::geometryChange(new_geometry, old_geometry);
  qInfo().nospace() << "trace preview.resize thread="
                    << QThread::currentThreadId() << " logical="
                    << new_geometry.width() << 'x' << new_geometry.height()
                    << " dpr="
                    << (window() == nullptr ? 1.0
                                            : window()->effectiveDevicePixelRatio());
  update();
}

void PreviewItem::attachWindowSignals() {
  QQuickWindow *quick_window = window();
  if (quick_window == nullptr || after_rendering_connection_) return;
  after_rendering_connection_ =
      connect(quick_window, &QQuickWindow::afterRendering, this,
              [this] { onAfterRendering(); }, Qt::DirectConnection);
  invalidated_connection_ =
      connect(quick_window, &QQuickWindow::sceneGraphInvalidated, this,
              [this] {
                render_node_ = nullptr;
                ++device_generation_;
                // A reconstructed scene graph belongs to a new native-device
                // generation even when the project revision/epoch is
                // unchanged. Re-arm the synthetic preview so it cannot reuse
                // the previous generation's completion decision.
                last_context_ = {};
                self_test_generation_done_ = false;
                pump_active_.store(true);
                qInfo().nospace()
                    << "trace preview.scene-graph.invalidated thread="
                    << QThread::currentThreadId() << " next-device="
                    << device_generation_;
                QMetaObject::invokeMethod(
                    this, [this] { emit sceneGraphTeardownObserved(); },
                    Qt::QueuedConnection);
              },
              Qt::DirectConnection);
}

void PreviewItem::onAfterRendering() {
#if defined(STITCH_EXPECT_METAL)
  PreviewNode *node = render_node_;
  if (node == nullptr || node->completionAttached() || node->hasFailed())
    return;
  QQuickWindow *quick_window = window();
  id<MTLCommandBuffer> command_buffer =
      (id<MTLCommandBuffer>)quick_window->rendererInterface()->getResource(
          quick_window, QSGRendererInterface::CommandListResource);
  if (command_buffer == nil) {
    node->failAfterSubmit(
        QStringLiteral("Qt Metal CommandListResource is unavailable"));
    return;
  }
  if (!node->attachCompletion(command_buffer)) return;
  qInfo().nospace() << "trace gpu.completion.attach thread="
                    << QThread::currentThreadId() << " device="
                    << device_generation_ << " epoch="
                    << node->frameContext().epoch << " lease=" << node->lease();
#endif
}

void PreviewItem::queueFailure(const QString &message) {
  QMetaObject::invokeMethod(
      this, [this, message] { emit lifecycleFailed(message); },
      Qt::QueuedConnection);
}
