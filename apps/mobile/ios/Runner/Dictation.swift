import AVFoundation
import Flutter
import Speech

/// Dictation on the iPhone: speech to text that never leaves the phone.
///
/// The Dart side is `lib/core/voice/platform_dictation.dart` (it documents the wire). Engines:
/// - iOS 26+: `SpeechAnalyzer` with a `SpeechTranscriber` (on-device by construction: Apple says its transcriber
///   modules do not send audio to Apple, and it needs no Speech Recognition permission, only the microphone).
/// - Older iOS: `SFSpeechRecognizer` with `requiresOnDeviceRecognition = true`. A locale whose recognizer has
///   `supportsOnDeviceRecognition == false` is reported as unsupported; this class never falls back to Apple's
///   servers by itself. `online: true` (the person's opt-in, asked by the app with its label) lifts the
///   requirement, and only then.
final class Dictation: NSObject, FlutterStreamHandler {
  private let methods: FlutterMethodChannel
  private let events: FlutterEventChannel
  private var sink: FlutterEventSink?

  /// One session at a time; `generation` makes a late callback of a finished session harmless.
  private var generation = 0
  private var session: Session?

  init(messenger: FlutterBinaryMessenger) {
    let name = "br.com.codeatlas.atlas/dictation"
    methods = FlutterMethodChannel(name: name, binaryMessenger: messenger)
    events = FlutterEventChannel(name: "\(name)/events", binaryMessenger: messenger)
    super.init()
    methods.setMethodCallHandler { [weak self] call, result in self?.handle(call, result) }
    events.setStreamHandler(self)
    NotificationCenter.default.addObserver(
      self, selector: #selector(interrupted(_:)),
      name: AVAudioSession.interruptionNotification, object: nil)
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

  /// Sends [event] to Dart if its session is still the current one. A final text ends the session (the
  /// microphone is already off); an error ends it and releases the microphone.
  private func emit(_ gen: Int, _ event: [String: Any]) {
    DispatchQueue.main.async { [weak self] in
      guard let self, gen == self.generation else { return }
      switch event["type"] as? String {
      case "final":
        self.session = nil
      case "error":
        self.session?.cancel()
        self.session = nil
      default:
        break
      }
      self.sink?(event)
    }
  }

  @objc private func interrupted(_ note: Notification) {
    guard let raw = note.userInfo?[AVAudioSessionInterruptionTypeKey] as? UInt,
      AVAudioSession.InterruptionType(rawValue: raw) == .began, let current = session
    else { return }
    let gen = generation
    current.cancel()
    session = nil
    emit(gen, ["type": "error", "code": "interrupted"])
  }

  private func handle(_ call: FlutterMethodCall, _ result: @escaping FlutterResult) {
    let args = call.arguments as? [String: Any] ?? [:]
    let tag = args["locale"] as? String ?? "en-US"
    switch call.method {
    case "support":
      Task { result(await Dictation.support(tag)) }
    case "start":
      let online = args["online"] as? Bool ?? false
      Task { @MainActor in
        do {
          try await self.start(tag: tag, online: online)
          result(nil)
        } catch let e as DictationError {
          result(FlutterError(code: e.code, message: e.message, details: nil))
        } catch {
          result(FlutterError(code: "failed", message: "\(error)", details: nil))
        }
      }
    case "stop":
      let gen = generation
      session?.stop { [weak self] text in
        self?.emit(gen, ["type": "final", "text": text])
      }
      result(nil)
    case "cancel":
      generation += 1
      session?.cancel()
      session = nil
      result(nil)
    default:
      result(FlutterMethodNotImplemented)
    }
  }

  private static var microphoneGranted: Bool {
    if #available(iOS 17.0, *) { return AVAudioApplication.shared.recordPermission == .granted }
    return AVAudioSession.sharedInstance().recordPermission == .granted
  }

  // MARK: support

  /// What this iPhone can do for [tag]; never records and never prompts.
  static func support(_ tag: String) async -> [String: Any] {
    let locale = Locale(identifier: tag)
    let classic = SFSpeechRecognizer(locale: locale)
    let online = classic?.isAvailable ?? false
    if #available(iOS 26.0, *), SpeechTranscriber.isAvailable,
      await SpeechTranscriber.supportedLocale(equivalentTo: locale) != nil
    {
      return [
        "on_device": "apple", "online": online, "speech_permission": false, "downloading": false,
      ]
    }
    if let classic, classic.supportsOnDeviceRecognition {
      return [
        "on_device": "apple", "online": online, "speech_permission": true, "downloading": false,
      ]
    }
    return [
      "online": online, "speech_permission": true, "downloading": false,
    ]
  }

  // MARK: start

  @MainActor
  private func start(tag: String, online: Bool) async throws {
    guard session == nil else { throw DictationError("busy", "a dictation is already running") }
    let locale = Locale(identifier: tag)
    generation += 1
    let gen = generation
    let report: (String, Bool) -> Void = { [weak self] text, final in
      self?.emit(gen, ["type": final ? "final" : "partial", "text": text])
    }
    let fail: (String) -> Void = { [weak self] code in
      self?.emit(gen, ["type": "error", "code": code])
    }
    guard Dictation.microphoneGranted else {
      throw DictationError("permission", "microphone permission is not granted")
    }
    if !online, #available(iOS 26.0, *), SpeechTranscriber.isAvailable,
      let supported = await SpeechTranscriber.supportedLocale(equivalentTo: locale)
    {
      let next = AnalyzerSession(locale: supported, report: report, fail: fail)
      try await next.start()
      session = next
      return
    }
    let next = ClassicSession(locale: locale, onDevice: !online, report: report, fail: fail)
    try next.start()
    session = next
  }
}

struct DictationError: Error {
  let code: String
  let message: String
  init(_ code: String, _ message: String) {
    self.code = code
    self.message = message
  }
}

/// A running session. `stop` finishes what was heard and reports the final text; `cancel` drops everything.
protocol Session: AnyObject {
  func stop(_ done: @escaping (String) -> Void)
  func cancel()
}

/// The microphone, tapped into [onBuffer]. Released by `stop()`; the audio session goes back to the others.
final class Microphone {
  private let engine = AVAudioEngine()

  func start(onBuffer: @escaping (AVAudioPCMBuffer) -> Void) throws {
    let audio = AVAudioSession.sharedInstance()
    do {
      try audio.setCategory(.record, mode: .measurement, options: [.duckOthers])
      try audio.setActive(true, options: [])
    } catch {
      throw DictationError("busy", "the microphone is in use: \(error.localizedDescription)")
    }
    let input = engine.inputNode
    let format = input.outputFormat(forBus: 0)
    guard format.channelCount > 0 else {
      throw DictationError("failed", "this iPhone has no microphone input")
    }
    input.installTap(onBus: 0, bufferSize: 2048, format: format) { buffer, _ in onBuffer(buffer) }
    engine.prepare()
    do {
      try engine.start()
    } catch {
      input.removeTap(onBus: 0)
      throw DictationError("busy", "could not start the microphone: \(error.localizedDescription)")
    }
  }

  func stop() {
    if engine.isRunning { engine.stop() }
    engine.inputNode.removeTap(onBus: 0)
    try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
  }
}

// MARK: iOS 26: SpeechAnalyzer

@available(iOS 26.0, *)
final class AnalyzerSession: Session {
  private let locale: Locale
  private let report: (String, Bool) -> Void
  private let fail: (String) -> Void
  private let mic = Microphone()
  private var transcriber: SpeechTranscriber?
  private var analyzer: SpeechAnalyzer?
  private var input: AsyncStream<AnalyzerInput>.Continuation?
  private var results: Task<Void, Never>?
  private var finalized = ""
  private var volatile = ""

  init(locale: Locale, report: @escaping (String, Bool) -> Void, fail: @escaping (String) -> Void) {
    self.locale = locale
    self.report = report
    self.fail = fail
  }

  func start() async throws {
    let transcriber = SpeechTranscriber(
      locale: locale, transcriptionOptions: [], reportingOptions: [.volatileResults],
      attributeOptions: [])
    // The language pack is the OS's, installed once; this fetches a model, never audio.
    do {
      if let request = try await AssetInventory.assetInstallationRequest(supporting: [transcriber]) {
        try await request.downloadAndInstall()
      }
    } catch {
      throw DictationError("language", "the language pack is not available: \(error)")
    }
    guard
      let format = await SpeechAnalyzer.bestAvailableAudioFormat(compatibleWith: [transcriber])
    else { throw DictationError("language", "no audio format for this language") }
    let analyzer = SpeechAnalyzer(modules: [transcriber])
    let (stream, continuation) = AsyncStream.makeStream(of: AnalyzerInput.self)
    self.transcriber = transcriber
    self.analyzer = analyzer
    input = continuation

    results = Task { [weak self] in
      do {
        for try await result in transcriber.results {
          guard let self else { return }
          let text = String(result.text.characters)
          if result.isFinal {
            self.finalized += (self.finalized.isEmpty ? "" : " ") + text.trimmingCharacters(in: .whitespaces)
            self.volatile = ""
          } else {
            self.volatile = text
          }
          self.report(self.transcript, false)
        }
      } catch {
        self?.fail("failed")
      }
    }
    try await analyzer.start(inputSequence: stream)

    let converter = BufferConverter(to: format)
    do {
      try mic.start { [weak self] buffer in
        guard let converted = converter.convert(buffer) else { return }
        self?.input?.yield(AnalyzerInput(buffer: converted))
      }
    } catch {
      continuation.finish()
      await analyzer.cancelAndFinishNow()
      results?.cancel()
      throw error
    }
  }

  private var transcript: String {
    [finalized, volatile.trimmingCharacters(in: .whitespaces)].filter { !$0.isEmpty }.joined(separator: " ")
  }

  func stop(_ done: @escaping (String) -> Void) {
    mic.stop()
    input?.finish()
    Task { [weak self] in
      guard let self else { return }
      try? await self.analyzer?.finalizeAndFinishThroughEndOfInput()
      await self.results?.value
      done(self.transcript)
    }
  }

  func cancel() {
    mic.stop()
    input?.finish()
    results?.cancel()
    Task { [analyzer] in await analyzer?.cancelAndFinishNow() }
  }
}

/// Converts what the microphone gives to the format the analyzer wants. The converter is rebuilt when the input
/// format changes (a route change: headphones in or out).
final class BufferConverter {
  private let output: AVAudioFormat
  private var converter: AVAudioConverter?
  private var from: AVAudioFormat?
  private let lock = NSLock()

  init(to output: AVAudioFormat) { self.output = output }

  func convert(_ buffer: AVAudioPCMBuffer) -> AVAudioPCMBuffer? {
    lock.lock()
    defer { lock.unlock() }
    if buffer.format == output { return buffer }
    if converter == nil || from != buffer.format {
      converter = AVAudioConverter(from: buffer.format, to: output)
      converter?.primeMethod = .none
      from = buffer.format
    }
    guard let converter else { return nil }
    let ratio = output.sampleRate / buffer.format.sampleRate
    let capacity = AVAudioFrameCount((Double(buffer.frameLength) * ratio).rounded(.up))
    guard let out = AVAudioPCMBuffer(pcmFormat: output, frameCapacity: max(capacity, 1)) else {
      return nil
    }
    var consumed = false
    var error: NSError?
    let status = converter.convert(to: out, error: &error) { _, inputStatus in
      if consumed {
        inputStatus.pointee = .noDataNow
        return nil
      }
      consumed = true
      inputStatus.pointee = .haveData
      return buffer
    }
    return status == .error ? nil : out
  }
}

// MARK: iOS 15 to 25: SFSpeechRecognizer

final class ClassicSession: Session {
  private let recognizer: SFSpeechRecognizer?
  private let onDevice: Bool
  private let report: (String, Bool) -> Void
  private let fail: (String) -> Void
  private let mic = Microphone()
  private var request: SFSpeechAudioBufferRecognitionRequest?
  private var task: SFSpeechRecognitionTask?
  private var last = ""
  private var stopping: ((String) -> Void)?

  init(
    locale: Locale, onDevice: Bool, report: @escaping (String, Bool) -> Void,
    fail: @escaping (String) -> Void
  ) {
    recognizer = SFSpeechRecognizer(locale: locale)
    self.onDevice = onDevice
    self.report = report
    self.fail = fail
  }

  func start() throws {
    guard SFSpeechRecognizer.authorizationStatus() == .authorized else {
      throw DictationError("permission", "speech recognition permission is not granted")
    }
    guard let recognizer, recognizer.isAvailable else {
      throw DictationError("language", "no recognizer for this language")
    }
    // The promise: on-device, or the person's explicit opt-in. Never a silent server fallback.
    if onDevice && !recognizer.supportsOnDeviceRecognition {
      throw DictationError("language", "this language cannot be recognised on this iPhone")
    }
    let request = SFSpeechAudioBufferRecognitionRequest()
    request.shouldReportPartialResults = true
    request.requiresOnDeviceRecognition = onDevice
    request.taskHint = .dictation
    if #available(iOS 16.0, *) { request.addsPunctuation = true }
    self.request = request
    task = recognizer.recognitionTask(with: request) { [weak self] result, error in
      guard let self else { return }
      if let result {
        self.last = result.bestTranscription.formattedString
        if result.isFinal {
          self.finish()
        } else {
          self.report(self.last, false)
        }
      } else if error != nil {
        // Apple ends a silent or finished task with an error too; what was heard is kept.
        if self.stopping != nil || !self.last.isEmpty { self.finish() } else { self.fail("failed") }
      }
    }
    try mic.start { [weak self] buffer in self?.request?.append(buffer) }
  }

  private func finish() {
    let text = last
    mic.stop()
    if let done = stopping {
      stopping = nil
      done(text)
    } else {
      report(text, true)
    }
  }

  func stop(_ done: @escaping (String) -> Void) {
    stopping = done
    mic.stop()
    request?.endAudio()
    // A recognizer that never answers must not hold the person: the last partial is what there is.
    DispatchQueue.main.asyncAfter(deadline: .now() + 2) { [weak self] in
      guard let self, let done = self.stopping else { return }
      self.stopping = nil
      done(self.last)
    }
  }

  func cancel() {
    stopping = nil
    mic.stop()
    request?.endAudio()
    task?.cancel()
    task = nil
  }
}
