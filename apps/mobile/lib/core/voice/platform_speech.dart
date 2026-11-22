/// The real [SpeechSynthesizer]: a platform channel to the phone's own text-to-speech.
///
/// Why not a plugin: a reply is the person's work, and it must not be able to leave the phone to be spoken.
/// iOS `AVSpeechSynthesizer` is on-device. Android `TextToSpeech` may use a network voice of the engine; the
/// native side (`android/.../SpeechPlugin.kt`) only ever picks a voice that is installed and does not require
/// the network, and reports the language as unavailable when there is none.
///
/// The wire (channel `br.com.codeatlas.atlas/speech`, events on `…/speech/events`):
/// - `available {locale}` → bool (an installed on-device voice exists for the language)
/// - `speak {text, locale}` → null, or a `PlatformException`; replaces what is being said, system rate
/// - `stop {}` → null, no event
/// - events: `{type: "done"}` when the utterance ended, `{type: "error"}` when it could not be spoken
library;

import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/voice/speech.dart';

const _channel = 'br.com.codeatlas.atlas/speech';

class PlatformSpeech implements SpeechSynthesizer {
  PlatformSpeech({MethodChannel? methods, EventChannel? eventChannel})
    : _methods = methods ?? const MethodChannel(_channel),
      _eventChannel = eventChannel ?? const EventChannel('$_channel/events');

  final MethodChannel _methods;
  final EventChannel _eventChannel;
  final _events = StreamController<SpeechEvent>.broadcast(sync: true);
  StreamSubscription<dynamic>? _wire;

  void _listen() {
    _wire ??= _eventChannel.receiveBroadcastStream().listen(
      (raw) {
        final event = parseSpeechEvent(raw);
        if (event != null) _events.add(event);
      },
      onError: (Object e) {
        debugPrint('atlas-mobile: speech event stream: $e');
        _events.add(SpeechEvent.failed);
      },
    );
  }

  @override
  Stream<SpeechEvent> get events => _events.stream;

  @override
  Future<bool> available(String localeTag) async {
    _listen();
    try {
      return await _methods.invokeMethod<bool>('available', {
            'locale': localeTag,
          }) ??
          false;
    } on MissingPluginException {
      return false;
    } on PlatformException catch (e) {
      debugPrint('atlas-mobile: speech availability unreadable: $e');
      return false;
    }
  }

  @override
  Future<void> speak(String text, {required String localeTag}) async {
    _listen();
    await _methods.invokeMethod<void>('speak', {
      'text': text,
      'locale': localeTag,
    });
  }

  @override
  Future<void> stop() async {
    try {
      await _methods.invokeMethod<void>('stop');
    } on PlatformException catch (e) {
      debugPrint('atlas-mobile: speech stop: $e');
    } on MissingPluginException {
      // No voice, nothing speaks.
    }
  }

  Future<void> dispose() async {
    await _wire?.cancel();
    await _events.close();
  }
}

/// One native event; null for a shape this app does not know.
@visibleForTesting
SpeechEvent? parseSpeechEvent(Object? raw) {
  if (raw is! Map) return null;
  return switch (raw['type']) {
    'done' => SpeechEvent.done,
    'error' => SpeechEvent.failed,
    _ => null,
  };
}

final speechSynthesizerProvider = Provider<SpeechSynthesizer>((ref) {
  final s = PlatformSpeech();
  ref.onDispose(s.dispose);
  return s;
});

/// The app's voice: one controller for every screen, so any tap or microphone can silence it.
final speechControllerProvider = Provider<SpeechController>((ref) {
  final c = SpeechController(ref.watch(speechSynthesizerProvider));
  ref.onDispose(c.dispose);
  return c;
});

/// Whether this phone has an installed on-device voice for a language tag. False hides the toggle: a phone that
/// cannot read aloud is never offered it.
final speechAvailableProvider = FutureProvider.autoDispose.family<bool, String>(
  (ref, localeTag) => ref.watch(speechControllerProvider).available(localeTag),
);
