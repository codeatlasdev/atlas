import Flutter
import UIKit

@main
@objc class AppDelegate: FlutterAppDelegate, FlutterImplicitEngineDelegate {
  /// Dictation (lib/core/voice/): kept alive for the life of the app.
  private var dictation: Dictation?

  /// Reading replies aloud (lib/core/voice/speech.dart): kept alive for the life of the app.
  private var speech: SpeechReader?

  override func application(
    _ application: UIApplication,
    didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?
  ) -> Bool {
    return super.application(application, didFinishLaunchingWithOptions: launchOptions)
  }

  func didInitializeImplicitFlutterEngine(_ engineBridge: FlutterImplicitEngineBridge) {
    GeneratedPluginRegistrant.register(with: engineBridge.pluginRegistry)
    if let registrar = engineBridge.pluginRegistry.registrar(forPlugin: "AtlasDictation") {
      dictation = Dictation(messenger: registrar.messenger())
      speech = SpeechReader(messenger: registrar.messenger())
    }
  }
}
