/// The real [DictationRecognizer]: a platform channel to the phone's own speech recognizers.
///
/// Why not a plugin: the promise of this feature is that audio stays on the phone, and the popular plugin
/// (`speech_to_text` 7.5, read at its source) cannot keep it everywhere. On Android its `onDevice` option calls
/// `createOnDeviceSpeechRecognizer` only when `isOnDeviceRecognitionAvailable` says yes, and otherwise falls
/// back, silently, to `createSpeechRecognizer` (the default service, which may use the network), as it does on
/// Android 11 and older; the caller is not told which one ran. On iOS it has no `SpeechAnalyzer` (iOS 26), plays
/// its own start sound, and owns the audio engine. So the native side is ours and small
/// (`ios/Runner/Dictation*.swift`, `android/.../DictationPlugin.kt`): on-device engines only, and an engine that
/// cannot recognise the language on the phone reports `language` instead of falling back to a server.
///
/// The wire (channel `br.com.codeatlas.atlas/dictation`, events on `…/dictation/events`):
/// - `support {locale}` → `{on_device: "apple"|"android"|null, online: bool, speech_permission: bool, downloading: bool}`
/// - `start {locale, online}` → null, or a `PlatformException` whose code is a [DictationFailure] name
/// - `stop {}` → null; a `final` event follows. `cancel {}` → null, no event.
/// - events: `{type: "partial"|"final", text}` (the whole transcript so far) and `{type: "error", code}`
library;

import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/voice/dictation.dart';

const _channel = 'br.com.codeatlas.atlas/dictation';

class PlatformDictation implements DictationRecognizer {
  PlatformDictation({MethodChannel? methods, EventChannel? eventChannel})
    : _methods = methods ?? const MethodChannel(_channel),
      _eventChannel = eventChannel ?? const EventChannel('$_channel/events');

  final MethodChannel _methods;
  final EventChannel _eventChannel;
  final _events = StreamController<DictationEvent>.broadcast(sync: true);
  StreamSubscription<dynamic>? _wire;

  /// One subscription for the life of the app: native events are never lost to a late `listen`.
  void _listen() {
    _wire ??= _eventChannel.receiveBroadcastStream().listen(
      (raw) {
        final event = parseDictationEvent(raw);
        if (event != null) _events.add(event);
      },
      onError: (Object e) {
        debugPrint('atlas-mobile: dictation event stream: $e');
        _events.add(const DictationFailureEvent(DictationFailure.failed));
      },
    );
  }

  @override
  Stream<DictationEvent> get events => _events.stream;

  @override
  Future<DictationSupport> support(String localeTag) async {
    _listen();
    try {
      final r = await _methods.invokeMapMethod<String, Object?>('support', {
        'locale': localeTag,
      });
      return parseDictationSupport(r);
    } on MissingPluginException {
      return DictationSupport.none;
    } on PlatformException catch (e) {
      debugPrint('atlas-mobile: dictation support unreadable: $e');
      return DictationSupport.none;
    }
  }

  @override
  Future<void> start({required String localeTag, required bool online}) async {
    _listen();
    try {
      await _methods.invokeMethod<void>('start', {
        'locale': localeTag,
        'online': online,
      });
    } on PlatformException catch (e) {
      throw DictationException(DictationFailure.byCode(e.code), e.message);
    } on MissingPluginException {
      throw const DictationException(
        DictationFailure.failed,
        'no recognizer on this platform',
      );
    }
  }

  @override
  Future<void> stop() async {
    try {
      await _methods.invokeMethod<void>('stop');
    } on PlatformException catch (e) {
      debugPrint('atlas-mobile: dictation stop: $e');
    }
  }

  @override
  Future<void> cancel() async {
    try {
      await _methods.invokeMethod<void>('cancel');
    } on PlatformException catch (e) {
      debugPrint('atlas-mobile: dictation cancel: $e');
    } on MissingPluginException {
      // No recognizer, nothing holds the microphone.
    }
  }

  Future<void> dispose() async {
    await _wire?.cancel();
    await _events.close();
  }
}

/// What the native `support` call answers; anything unreadable is "nothing works", never a guess.
@visibleForTesting
DictationSupport parseDictationSupport(Map<String, Object?>? m) {
  if (m == null) return DictationSupport.none;
  return DictationSupport(
    onDevice: DictationEngine.byId(m['on_device']),
    online: m['online'] == true,
    needsSpeechPermission: m['speech_permission'] == true,
    downloading: m['downloading'] == true,
  );
}

/// One native event; null for a shape this app does not know.
@visibleForTesting
DictationEvent? parseDictationEvent(Object? raw) {
  if (raw is! Map) return null;
  return switch (raw['type']) {
    'partial' => DictationPartial(raw['text'] as String? ?? ''),
    'final' => DictationFinal(raw['text'] as String? ?? ''),
    'error' => DictationFailureEvent(DictationFailure.byCode(raw['code'])),
    _ => null,
  };
}

final dictationProvider = Provider<DictationRecognizer>((ref) {
  final d = PlatformDictation();
  ref.onDispose(d.dispose);
  return d;
});
