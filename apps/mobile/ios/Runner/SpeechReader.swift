import AVFoundation
import Flutter

/// Reading replies aloud on the iPhone: `AVSpeechSynthesizer`, which speaks on the phone with the voices installed
/// in the system (nothing of the text is sent anywhere).
///
/// The Dart side is `lib/core/voice/platform_speech.dart` (it documents the wire). The voice and the rate are the
/// system's: `prefersAssistiveTechnologySettings` makes the utterance follow Settings → Accessibility → Spoken
/// Content (voice, rate), and the language is the app's (`en-US` / `pt-BR`). A language with no installed voice
/// is an error (`language`), never another language.
final class SpeechReader: NSObject, FlutterStreamHandler, AVSpeechSynthesizerDelegate {
  private let methods: FlutterMethodChannel
  private let events: FlutterEventChannel
  private var sink: FlutterEventSink?
  private let synth = AVSpeechSynthesizer()

  /// The utterance Dart is waiting on. A callback of any other (replaced or stopped) is ignored.
  private var current: AVSpeechUtterance?

  init(messenger: FlutterBinaryMessenger) {
    let name = "br.com.codeatlas.atlas/speech"
    methods = FlutterMethodChannel(name: name, binaryMessenger: messenger)
    events = FlutterEventChannel(name: "\(name)/events", binaryMessenger: messenger)
    super.init()
    synth.delegate = self
    methods.setMethodCallHandler { [weak self] call, result in self?.handle(call, result) }
    events.setStreamHandler(self)
  }

  func onListen(withArguments arguments: Any?, eventSink events: @escaping FlutterEventSink)
    -> FlutterError?
  {
    sink = events
    return nil
  }

  func onCancel(withArguments arguments: Any?) -> FlutterError? {
    sink = nil
    return nil
  }

  private func handle(_ call: FlutterMethodCall, _ result: @escaping FlutterResult) {
    let args = call.arguments as? [String: Any]
    let tag = args?["locale"] as? String ?? "en-US"
    switch call.method {
    case "available":
      result(voice(for: tag) != nil)
    case "speak":
      guard let text = args?["text"] as? String, let voice = voice(for: tag) else {
        result(FlutterError(code: "language", message: "no voice for \(tag)", details: nil))
        return
      }
      speak(text, voice: voice)
      result(nil)
    case "stop":
      stop()
      result(nil)
    default:
      result(FlutterMethodNotImplemented)
    }
  }

  /// An installed voice for exactly [tag]; AVSpeechSynthesizer speaks on the device.
  private func voice(for tag: String) -> AVSpeechSynthesisVoice? {
    AVSpeechSynthesisVoice(language: tag).flatMap { $0.language == tag ? $0 : nil }
  }

  private func speak(_ text: String, voice: AVSpeechSynthesisVoice) {
    stop()
    let session = AVAudioSession.sharedInstance()
    try? session.setCategory(.playback, mode: .spokenAudio, options: [.duckOthers])
    try? session.setActive(true, options: [])
    let utterance = AVSpeechUtterance(string: text)
    utterance.voice = voice
    if #available(iOS 14.5, *) { utterance.prefersAssistiveTechnologySettings = true }
    current = utterance
    synth.speak(utterance)
  }

  /// Silences the voice at once; no event follows.
  private func stop() {
    current = nil
    if synth.isSpeaking { synth.stopSpeaking(at: .immediate) }
    try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
  }

  private func finish(_ utterance: AVSpeechUtterance) {
    guard utterance === current else { return }
    current = nil
    try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
    sink?(["type": "done"])
  }

  func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didFinish utterance: AVSpeechUtterance) {
    finish(utterance)
  }
}
