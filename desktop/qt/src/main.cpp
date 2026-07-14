#include <QGuiApplication>
#include <QQuickWindow>
#include <QSGRendererInterface>

namespace {

#if defined(STITCH_EXPECT_METAL)
constexpr auto kExpectedBackend = QSGRendererInterface::Metal;
constexpr const char *kExpectedBackendName = "Metal";
#elif defined(STITCH_EXPECT_D3D11)
constexpr auto kExpectedBackend = QSGRendererInterface::Direct3D11;
constexpr const char *kExpectedBackendName = "Direct3D11";
#else
#error "A supported platform backend must be selected by CMake."
#endif

void assert_backend(QQuickWindow &window) {
  const auto actual = window.rendererInterface()->graphicsApi();
  if (actual != kExpectedBackend) {
    qFatal("Required Qt backend %s was not initialized; refusing a copy-prone fallback.",
           kExpectedBackendName);
  }
}

}  // namespace

int main(int argc, char *argv[]) {
  QGuiApplication app(argc, argv);
  QQuickWindow::setGraphicsApi(kExpectedBackend);

  QQuickWindow window;
  window.setTitle("Stitch Editor bootstrap");
  window.resize(960, 540);
  QObject::connect(&window, &QQuickWindow::sceneGraphInitialized, &window,
                   [&window] { assert_backend(window); }, Qt::DirectConnection);
  window.show();

  return app.exec();
}
