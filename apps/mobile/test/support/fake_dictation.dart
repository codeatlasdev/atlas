import 'dart:async';

import 'package:atlas_mobile/core/voice/dictation.dart';

/// The platform's recognizer, scripted: what this phone supports, what a session says, and whether the
/// microphone is held right now ([micLive]), so a test can say "released" without a device.
class FakeDictation implements DictationRecognizer {
  FakeDictation({
    this.supports = const DictationSupport(onDevice: DictationEngine.apple),
  });

  /// What `support` answers; change it between taps.
  DictationSupport supports;

  /// Throw this from `start` (a refusal: busy, permission, language).
  DictationException? refuseStart;

  /// `start` completes only when this does (a recognizer that is slow to start).
  Completer<void>? startGate;

  /// What `stop` answers as the final text; null: the last partial is the final text.
  String? finalOnStop;

  final started = <({String locale, bool online})>[];
  final supportAsked = <String>[];
  var stops = 0;
  var cancels = 0;
  var micLive = false;
  String _last = '';

  final _events = StreamController<DictationEvent>.broadcast(sync: true);

  @override
  Stream<DictationEvent> get events => _events.stream;

  @override
  Future<DictationSupport> support(String localeTag) async {
    supportAsked.add(localeTag);
    return supports;
  }

  @override
  Future<void> start({required String localeTag, required bool online}) async {
    final refuse = refuseStart;
    if (refuse != null) throw refuse;
    micLive = true;
    started.add((locale: localeTag, online: online));
    await startGate?.future;
  }

  @override
  Future<void> stop() async {
    stops++;
    micLive = false;
    _events.add(DictationFinal(finalOnStop ?? _last));
  }

  @override
  Future<void> cancel() async {
    cancels++;
    micLive = false;
  }

  /// The recognizer heard [text] so far (the whole transcript of the session).
  void say(String text) {
    _last = text;
    _events.add(DictationPartial(text));
  }

  /// The engine ended the session by itself.
  void endByItself(String text) {
    _last = text;
    micLive = false;
    _events.add(DictationFinal(text));
  }

  void failWith(DictationFailure code) {
    micLive = false;
    _events.add(DictationFailureEvent(code));
  }
}
