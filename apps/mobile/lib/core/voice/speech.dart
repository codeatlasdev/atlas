/// Reading replies aloud: the phone's own text-to-speech, and the one controller every screen shares.
///
/// The voice is the system's (iOS `AVSpeechSynthesizer`, Android `TextToSpeech` with an installed voice) at the
/// system's rate, in the app's language (`dictationLocaleTag`), reached through the native channel in
/// `platform_speech.dart`. A plugin is not used for the same reason as in dictation: the text of a reply must not
/// be able to leave the phone, and a plugin cannot promise that for every Android engine. This file is plain
/// Dart: the seam ([SpeechSynthesizer]) tests drive with a fake, and [SpeechController], which holds the rules
/// that hold on every screen: one utterance at a time, silence the moment the microphone opens, a tap, or the app
/// goes to the background.
library;

import 'dart:async';

import 'package:flutter/widgets.dart';

/// How an utterance ended on its own (a stop that the app asked for sends nothing).
enum SpeechEvent { done, failed }

/// The one seam to the platform's text-to-speech. [PlatformSpeech] is the real one.
abstract interface class SpeechSynthesizer {
  /// Whether this phone has an installed, on-device voice for [localeTag] (`en-US`, `pt-BR`). Never speaks.
  Future<bool> available(String localeTag);

  /// Starts [text], replacing whatever is being said. Returns once it is queued; [events] says how it ended.
  Future<void> speak(String text, {required String localeTag});

  /// Silences the voice at once. No event follows.
  Future<void> stop();

  /// How the utterance in flight ended.
  Stream<SpeechEvent> get events;
}

/// The voice of the app: whether it speaks, and who may make it.
///
/// - One utterance at a time; a new [speak] replaces the old one.
/// - Nothing is spoken while a microphone is open ([claimMic]): the phone would hear itself, and a person
///   dictating is not listening. Opening a microphone silences the voice at once.
/// - The app leaving the foreground silences it at once ([didChangeAppLifecycleState]).
/// - Text that is empty after trimming says nothing.
class SpeechController extends ChangeNotifier with WidgetsBindingObserver {
  SpeechController(this._synth) {
    _sub = _synth.events.listen(_onEvent);
    WidgetsBinding.instance.addObserver(this);
  }

  final SpeechSynthesizer _synth;
  late final StreamSubscription<SpeechEvent> _sub;

  bool _speaking = false;
  bool _failed = false;
  Object? _micOwner;

  /// The voice is being heard (or about to be) right now.
  bool get speaking => _speaking;

  /// The last utterance could not be spoken; cleared by the next [speak].
  bool get failed => _failed;

  /// Whether a microphone (dictation, a voice answer) is open: nothing is spoken meanwhile.
  bool get micOpen => _micOwner != null;

  /// Whether the phone has a voice for [localeTag].
  Future<bool> available(String localeTag) => _synth.available(localeTag);

  /// Says [text] in [localeTag]. `false`: nothing was said (empty text, a microphone is open, or the app is not
  /// on screen: a reply that arrives in the background is not read to a pocket).
  Future<bool> speak(String text, String localeTag) async {
    final words = text.trim();
    if (words.isEmpty || micOpen || !_onScreen) return false;
    _failed = false;
    _set(speaking: true);
    try {
      await _synth.speak(words, localeTag: localeTag);
    } on Object catch (e) {
      debugPrint('atlas-mobile: speech failed to start: $e');
      _failed = true;
      _set(speaking: false);
      return false;
    }
    return true;
  }

  bool get _onScreen => switch (WidgetsBinding.instance.lifecycleState) {
    AppLifecycleState.hidden ||
    AppLifecycleState.paused ||
    AppLifecycleState.detached => false,
    _ => true,
  };

  /// Silences the voice at once (a tap, the toggle turned off, the screen closed). Safe when nothing speaks.
  Future<void> stop() async {
    if (!_speaking) return;
    _set(speaking: false);
    try {
      await _synth.stop();
    } on Object catch (e) {
      debugPrint('atlas-mobile: speech stop failed: $e');
    }
  }

  /// [owner] opens a microphone: the voice goes quiet at once. `false` when another owner already holds one, and
  /// nothing changes (two recognizers must never run together).
  bool claimMic(Object owner) {
    if (_micOwner != null && !identical(_micOwner, owner)) return false;
    _micOwner = owner;
    unawaited(stop());
    notifyListeners();
    return true;
  }

  /// [owner]'s microphone is closed. A release by someone who does not hold it changes nothing.
  void releaseMic(Object owner) {
    if (!identical(_micOwner, owner)) return;
    _micOwner = null;
    notifyListeners();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    switch (state) {
      case AppLifecycleState.hidden ||
          AppLifecycleState.paused ||
          AppLifecycleState.detached:
        unawaited(stop());
      case AppLifecycleState.resumed || AppLifecycleState.inactive:
        break;
    }
  }

  void _onEvent(SpeechEvent e) {
    if (!_speaking) return;
    if (e == SpeechEvent.failed) _failed = true;
    _set(speaking: false);
  }

  void _set({required bool speaking}) {
    _speaking = speaking;
    notifyListeners();
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    unawaited(_sub.cancel());
    if (_speaking) unawaited(_synth.stop());
    super.dispose();
  }
}
