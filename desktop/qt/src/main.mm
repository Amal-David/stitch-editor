#include "editor_controller.h"
#include "preview_item.h"

#include <QAccessible>
#include <QDebug>
#include <QGuiApplication>
#include <QQmlContext>
#include <QQuickGraphicsDevice>
#include <QQuickView>
#include <QSGRendererInterface>
#include <QThread>
#include <QTimer>

#include <atomic>
#include <array>

#if defined(STITCH_EXPECT_METAL)
#import <Metal/Metal.h>
#endif

namespace {

constexpr int kSelfTestTimeoutMs = 15000;

bool verifyAccessibility(QObject *root) {
  struct ExpectedAccessible {
    const char *object_name;
    QAccessible::Role role;
  };
  static constexpr std::array<ExpectedAccessible, 4> kExpected{{
      {"openSnapshotButton", QAccessible::Button},
      {"submitEditButton", QAccessible::Button},
      {"cancelPreviewButton", QAccessible::Button},
      {"nativePreview", QAccessible::Graphic},
  }};

  QAccessible::setActive(true);
  for (const ExpectedAccessible &expected : kExpected) {
    QObject *object = root->findChild<QObject *>(
        QString::fromLatin1(expected.object_name));
    QAccessibleInterface *interface =
        object == nullptr ? nullptr
                          : QAccessible::queryAccessibleInterface(object);
    if (interface == nullptr || interface->role() != expected.role ||
        interface->text(QAccessible::Name).trimmed().isEmpty()) {
      qCritical().nospace()
          << "trace accessibility.failure object=" << expected.object_name;
      return false;
    }
    qInfo().nospace() << "trace accessibility.verify object="
                      << expected.object_name << " role="
                      << static_cast<uint32_t>(interface->role())
                      << " name=present";
  }
  return true;
}

}  // namespace

int main(int argc, char **argv) {
  QGuiApplication app(argc, argv);
  app.setApplicationName(QStringLiteral("Stitch Editor"));
  const bool self_test_visible =
      app.arguments().contains(QStringLiteral("--self-test-visible"));
  const bool self_test = self_test_visible ||
      app.arguments().contains(QStringLiteral("--self-test"));
  if (self_test) app.setQuitOnLastWindowClosed(false);

#if defined(STITCH_EXPECT_METAL)
  id<MTLDevice> adopted_device = MTLCreateSystemDefaultDevice();
  if (adopted_device == nil) {
    qCritical() << "trace bootstrap.failure Metal device creation failed";
    return 2;
  }
  id<MTLCommandQueue> adopted_queue = [adopted_device newCommandQueue];
  if (adopted_queue == nil) {
    qCritical() << "trace bootstrap.failure Metal command queue creation failed";
    [adopted_device release];
    return 2;
  }
  QQuickWindow::setGraphicsApi(QSGRendererInterface::Metal);
  const QQuickGraphicsDevice graphics_device =
      QQuickGraphicsDevice::fromDeviceAndCommandQueue(
          (MTLDevice *)adopted_device, (MTLCommandQueue *)adopted_queue);
#else
  QQuickWindow::setGraphicsApi(QSGRendererInterface::Direct3D11);
#endif

  int exit_code = 2;
  std::atomic_bool teardown_observed{false};
  bool pre_teardown_verified = false;
  {
    EditorController controller;
    QQuickView view;
    view.setTitle(QStringLiteral("Stitch Editor"));
    view.resize(960, 540);
    if (self_test && !self_test_visible) {
      // An automated run still needs a real native window and swapchain to
      // prove the adopted-device scene-graph path. Keep that window fully
      // transparent, unable to activate, and transparent to input so
      // repetition does not disrupt the user's session. Use
      // --self-test-visible only for an intentional visual diagnostic.
      view.setFlag(Qt::Tool, true);
      view.setFlag(Qt::WindowDoesNotAcceptFocus, true);
      view.setFlag(Qt::WindowTransparentForInput, true);
      view.setOpacity(0.0);
      qInfo() << "trace self-test.presentation hidden-nonactivating";
    }
    view.setResizeMode(QQuickView::SizeRootObjectToView);
    view.setPersistentGraphics(false);
    view.setPersistentSceneGraph(false);
#if defined(STITCH_EXPECT_METAL)
    // The adopted device and queue are installed before QML source loading or
    // scene-graph initialization.
    view.setGraphicsDevice(graphics_device);
#endif

    std::atomic_bool backend_verified{false};
    std::atomic_bool dpr_verified{false};
    QObject::connect(
        &view, &QQuickWindow::sceneGraphInvalidated, &view,
        [&teardown_observed] {
          teardown_observed.store(true);
          qInfo().nospace() << "trace self-test.teardown.direct thread="
                            << QThread::currentThreadId();
        },
        Qt::DirectConnection);
    QObject::connect(
        &view, &QQuickWindow::sceneGraphInitialized, &view,
        [&view, &backend_verified, &dpr_verified
#if defined(STITCH_EXPECT_METAL)
         , adopted_device, adopted_queue
#endif
    ] {
#if defined(STITCH_EXPECT_METAL)
          QSGRendererInterface *renderer = view.rendererInterface();
          void *device = renderer->getResource(
              &view, QSGRendererInterface::DeviceResource);
          void *queue = renderer->getResource(
              &view, QSGRendererInterface::CommandQueueResource);
          if (renderer->graphicsApi() != QSGRendererInterface::Metal ||
              device != adopted_device || queue != adopted_queue) {
            qFatal("Metal backend or adopted device/queue identity mismatch");
          }
          qInfo().nospace()
              << "trace bootstrap.backend thread="
              << QThread::currentThreadId()
              << " api=Metal device-identity=adopted queue-identity=adopted";
#else
          if (view.rendererInterface()->graphicsApi() !=
              QSGRendererInterface::Direct3D11) {
            qFatal("Direct3D11 backend mismatch");
          }
#endif
          const qreal dpr = view.effectiveDevicePixelRatio();
          if (!(dpr > 0.0)) qFatal("Invalid device pixel ratio");
          qInfo().nospace() << "trace preview.dpr.after-init thread="
                            << QThread::currentThreadId() << " dpr=" << dpr;
          dpr_verified.store(true);
          backend_verified.store(true);
        },
        Qt::DirectConnection);

    QObject::connect(&view, &QQuickWindow::devicePixelRatioChanged, &view,
                     [&view] {
                       qInfo().nospace()
                           << "trace preview.dpr thread="
                           << QThread::currentThreadId() << " dpr="
                           << view.effectiveDevicePixelRatio();
                     });

    qmlRegisterType<PreviewItem>("StitchShell", 1, 0, "PreviewItem");
    view.rootContext()->setContextProperty(QStringLiteral("shell"),
                                           &controller);
    view.setSource(QUrl(QStringLiteral(
        "qrc:/qt/qml/StitchShell/src/Main.qml")));
    if (view.status() != QQuickView::Ready || view.rootObject() == nullptr) {
      for (const QQmlError &error : view.errors()) qCritical() << error;
      qCritical() << "trace bootstrap.failure QML source did not load";
      exit_code = 3;
    } else {
      auto *preview = view.rootObject()->findChild<PreviewItem *>(
          QStringLiteral("nativePreview"));
      if (preview == nullptr || !controller.isReady()) {
        qCritical() << "trace bootstrap.failure shell binding unavailable";
        exit_code = 3;
      } else {
        preview->setSelfTestMode(self_test);
#if defined(STITCH_EXPECT_METAL)
        preview->setExpectedGraphicsDevice(adopted_device);
#endif
        preview->setController(&controller);

        bool cancellation_observed = false;
        bool retirement_observed = false;
        bool lifecycle_failed = false;
        bool lease_submitted_observed = false;
        bool post_submit_resize_observed = false;
        bool accessibility_verified = false;
        bool presentation_verified = self_test_visible;
        QObject::connect(&view, &QWindow::widthChanged, &view, [&](int width) {
          if (!self_test || !lease_submitted_observed) return;
          post_submit_resize_observed = true;
          qInfo().nospace()
              << "trace self-test.resize-after-submit thread="
              << QThread::currentThreadId() << " logical=" << width << 'x'
              << view.height() << " dpr=" << view.effectiveDevicePixelRatio();
        });
        QObject::connect(
            preview, &PreviewItem::lifecycleFailed, &view,
            [&](const QString &message) {
              lifecycle_failed = true;
              qCritical().nospace()
                  << "trace self-test.failure lifecycle=" << message;
              if (self_test) {
                app.exit(4);
              }
            });
        QObject::connect(
            preview, &PreviewItem::leaseSubmitted, &view,
            [&](qulonglong lease, qulonglong submitted_epoch) {
              if (!self_test || cancellation_observed) return;
              lease_submitted_observed = true;
              view.resize(view.width() + 64, view.height() + 36);
              qInfo().nospace()
                  << "trace self-test.cancel-after-submit thread="
                  << QThread::currentThreadId() << " epoch="
                  << submitted_epoch << " lease=" << lease;
              cancellation_observed = controller.cancelEpoch();
              if (!cancellation_observed) {
                qCritical() << "trace self-test.failure epoch cancellation";
                app.exit(5);
              }
            });
        QObject::connect(
            preview, &PreviewItem::leaseRetired, &view,
            [&](qulonglong lease) {
              retirement_observed = true;
              qInfo().nospace()
                  << "trace self-test.retired thread="
                  << QThread::currentThreadId() << " lease=" << lease;
              if (self_test) {
                pre_teardown_verified = backend_verified.load() &&
                    dpr_verified.load() && accessibility_verified &&
                    presentation_verified &&
                    post_submit_resize_observed && cancellation_observed &&
                    retirement_observed && !lifecycle_failed;
                app.exit(pre_teardown_verified ? 0 : 8);
              }
            });
        QObject::connect(
            preview, &PreviewItem::sceneGraphTeardownObserved, &view, [&] {
              qInfo().nospace()
                  << "trace self-test.teardown thread="
                  << QThread::currentThreadId();
            });

        if (self_test) {
          QTimer::singleShot(0, &view, [&] {
            if (!self_test_visible) {
              presentation_verified = qFuzzyIsNull(view.opacity()) &&
                  view.flags().testFlag(Qt::WindowDoesNotAcceptFocus) &&
                  view.flags().testFlag(Qt::WindowTransparentForInput) &&
                  !view.isActive() && QGuiApplication::focusWindow() != &view;
              qInfo().nospace()
                  << "trace self-test.presentation.verify hidden="
                  << presentation_verified << " opacity=" << view.opacity()
                  << " active=" << view.isActive() << " position="
                  << view.position();
            }
            accessibility_verified = verifyAccessibility(view.rootObject());
            if (!presentation_verified || !accessibility_verified ||
                !controller.openSnapshot() ||
                !controller.submitDemoBatch()) {
              qCritical()
                  << "trace self-test.failure presentation/accessibility/"
                     "snapshot/edit";
              app.exit(6);
            }
          });
          QTimer::singleShot(kSelfTestTimeoutMs, &view, [&] {
            qCritical().nospace()
                << "trace self-test.timeout backend="
                << backend_verified.load() << " cancel="
                << cancellation_observed << " retire="
                << retirement_observed << " dpr=" << dpr_verified.load()
                << " accessibility=" << accessibility_verified
                << " presentation=" << presentation_verified
                << " post-submit-resize=" << post_submit_resize_observed
                << " lifecycle-failed=" << lifecycle_failed;
            app.exit(124);
          });
        }
        view.show();
        exit_code = app.exec();
      }
    }
  }

  if (self_test && exit_code == 0) {
    if (pre_teardown_verified && teardown_observed.load()) {
      qInfo() << "trace self-test.success";
    } else {
      qCritical().nospace()
          << "trace self-test.failure teardown preconditions="
          << pre_teardown_verified
          << " invalidated-during-destruction=" << teardown_observed.load();
      exit_code = 9;
    }
  }

#if defined(STITCH_EXPECT_METAL)
  // QQuickView and its scene graph are gone before the adopted native objects.
  [adopted_queue release];
  [adopted_device release];
#endif
  return exit_code;
}
