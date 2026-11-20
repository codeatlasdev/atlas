import 'dart:async';

import 'package:atlas_mobile/core/voice/read_aloud.dart';
import 'package:atlas_mobile/core/voice/speech.dart';

/// The phone's text-to-speech, scripted: what it was asked to say, whether a voice is "heard" right now
/// ([audible]) and how many times it was silenced.
class FakeSpeech implements SpeechSynthesizer {
  FakeSpeech({this.hasVoice = true});

  /// What `available` answers.
  bool hasVoice;

  /// Throw this from `speak` (a refusal: no voice, engine down).
  Object? refuseSpeak;

  final spoken = <({String text, String locale})>[];
  var stops = 0;
  var audible = false;

  final _events = StreamController<SpeechEvent>.broadcast(sync: true);

  @override
  Stream<SpeechEvent> get events => _events.stream;

  @override
  Future<bool> available(String localeTag) async => hasVoice;

  @override
  Future<void> speak(String text, {required String localeTag}) async {
    final refuse = refuseSpeak;
    if (refuse != null) throw refuse;
    audible = true;
    spoken.add((text: text, locale: localeTag));
  }

  @override
  Future<void> stop() async {
    stops++;
    audible = false;
  }

  /// The utterance ran to its end by itself.
  void finish() {
    audible = false;
    _events.add(SpeechEvent.done);
  }

  void fail() {
    audible = false;
    _events.add(SpeechEvent.failed);
  }
}

/// The per-phone switch kept in memory.
class MemoryReadAloudStore implements ReadAloudStore {
  MemoryReadAloudStore({this.on = false});
  bool on;

  @override
  Future<bool> load() async => on;

  @override
  Future<void> save(bool value) async => on = value;
}
