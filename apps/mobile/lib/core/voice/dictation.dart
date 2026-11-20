/// Dictation on the phone: speech becomes text in the composer, and the audio never leaves the phone.
///
/// The recognizer is the phone's own (iOS 26 `SpeechAnalyzer`, older iOS `SFSpeechRecognizer` with
/// `requiresOnDeviceRecognition`, Android 12+ `createOnDeviceSpeechRecognizer`), reached through the native
/// channel in `platform_dictation.dart`. A plugin is not used because none can promise on-device: see
/// docs/mobile.md ("Dictation"). hostd is not involved in recognising; it only learns that a task was
/// `dictated` and whether the crew allows voice (`crew_voice_status.enabled`, `core/protocol/crew_voice.dart`).
///
/// This file is plain Dart: the seam ([DictationRecognizer]) the screens and tests use, and the one rule for
/// where dictated text goes ([insertDictation]).
library;

import 'package:flutter/services.dart' show TextEditingValue, TextSelection;
import 'package:flutter/widgets.dart' show Locale;

/// Dictation stops by itself after this long and keeps what was said (hostd's cap is the same 300 s).
const dictationCap = Duration(minutes: 5);

/// Which recognizer a session uses. The person sees it all the time ([DictationEngine.id] is the native name).
enum DictationEngine {
  /// iOS: `SpeechAnalyzer` (26+) or `SFSpeechRecognizer` with `requiresOnDeviceRecognition`.
  apple('apple'),

  /// Android 12+: `SpeechRecognizer.createOnDeviceSpeechRecognizer`.
  android('android'),

  /// The platform's own service (`SFSpeechRecognizer` without the on-device flag, `createSpeechRecognizer`):
  /// it MAY send audio to Apple or Google. Only ever used after an explicit opt-in.
  online('online');

  const DictationEngine(this.id);
  final String id;

  bool get onDevice => this != online;

  static DictationEngine? byId(Object? id) {
    for (final e in values) {
      if (e.id == id) return e;
    }
    return null;
  }
}

/// What this phone can do for [DictationRecognizer.support], for one language.
class DictationSupport {
  const DictationSupport({
    this.onDevice,
    this.online = false,
    this.needsSpeechPermission = false,
    this.downloading = false,
  });

  /// The on-device engine, or null when this phone has none for the language.
  final DictationEngine? onDevice;

  /// Whether the system recognizer (which may use the network) exists. Only an opt-in uses it.
  final bool online;

  /// iOS recognizers other than `SpeechAnalyzer` need the Speech Recognition permission on top of the
  /// microphone.
  final bool needsSpeechPermission;

  /// Android is still installing the on-device language pack; asking again in a moment works.
  final bool downloading;

  /// An empty answer: nothing works on this phone.
  static const none = DictationSupport();

  /// The engine a session would run: on-device when there is one, else the system recognizer when [allowOnline].
  DictationEngine? engine({required bool allowOnline}) =>
      onDevice ?? (allowOnline && online ? DictationEngine.online : null);
}

/// Everything a recognizer reports while a session runs. [DictationPartial.text] and [DictationFinal.text] are
/// the whole transcript of the session so far, never a delta.
sealed class DictationEvent {
  const DictationEvent();
}

class DictationPartial extends DictationEvent {
  const DictationPartial(this.text);
  final String text;
}

/// The session ended (asked for, or the engine ran out) with this text. Nothing follows.
class DictationFinal extends DictationEvent {
  const DictationFinal(this.text);
  final String text;
}

/// The session failed. [code] is one of [DictationFailure]; the text said so far is not lost: the screen keeps
/// the last [DictationPartial].
class DictationFailureEvent extends DictationEvent {
  const DictationFailureEvent(this.code);
  final DictationFailure code;
}

/// Why dictation could not start or stopped, in the vocabulary of the native side (`code` of its errors).
enum DictationFailure {
  /// The microphone or speech permission is not (or no longer) granted.
  permission,

  /// Another app holds the microphone.
  busy,

  /// A call or the system took the microphone.
  interrupted,

  /// No model for the language, on-device (never retried on the network).
  language,

  /// Anything else.
  failed;

  static DictationFailure byCode(Object? code) => switch (code) {
    'permission' => permission,
    'busy' => busy,
    'interrupted' => interrupted,
    'language' => language,
    _ => failed,
  };
}

/// A refusal to start: the recognizer says why and nothing was recorded.
class DictationException implements Exception {
  const DictationException(this.code, [this.detail]);
  final DictationFailure code;
  final String? detail;

  @override
  String toString() =>
      'DictationException(${code.name}${detail == null ? '' : ': $detail'})';
}

/// The one seam to the platform's recognizer. [PlatformDictation] is the real one; tests drive the real screens
/// with a fake.
abstract interface class DictationRecognizer {
  /// What this phone can do for [localeTag] (`en-US`, `pt-BR`). Never records and never prompts.
  Future<DictationSupport> support(String localeTag);

  /// Starts a session. [online] false is a promise: the audio stays on the phone, and an engine that cannot do
  /// that for the language throws [DictationException] instead of falling back. Events follow on [events];
  /// the permissions were asked for before this call.
  Future<void> start({required String localeTag, required bool online});

  /// Ends the session gracefully: the recognizer finishes what it heard and a [DictationFinal] follows.
  Future<void> stop();

  /// Releases the microphone at once. No event follows.
  Future<void> cancel();

  /// Events of the running session (one at a time).
  Stream<DictationEvent> get events;
}

/// The speech language for the app's [locale]: Portuguese is Brazilian (the only Portuguese Atlas has), every
/// other language of the app is English.
String dictationLocaleTag(Locale locale) =>
    locale.languageCode == 'pt' ? 'pt-BR' : 'en-US';

/// [draft] with [dictated] put where the cursor is, or in place of the selection, with the spaces fixed so the
/// new words never glue to the old ones and never double a space (punctuation attaches to the word before it);
/// the cursor lands after them. A draft with no valid cursor takes the words at its end. [dictated] is trimmed;
/// empty text changes nothing.
TextEditingValue insertDictation(TextEditingValue draft, String dictated) =>
    spliceDictation(draft, dictated).value;

/// [insertDictation] and where the inserted run is in the result: `value.text.substring(from, to)` is the
/// words with the spaces around them, so a preview can draw them apart from the draft.
({TextEditingValue value, int from, int to}) spliceDictation(
  TextEditingValue draft,
  String dictated,
) {
  final words = dictated.trim();
  final text = draft.text;
  final sel = draft.selection;
  final start = sel.isValid ? sel.start.clamp(0, text.length) : text.length;
  final end = sel.isValid ? sel.end.clamp(start, text.length) : text.length;
  if (words.isEmpty) return (value: draft, from: start, to: start);
  final before = text.substring(0, start);
  final after = text.substring(end);
  final lead =
      before.isNotEmpty &&
          !_isSpace(before[before.length - 1]) &&
          !_closes(words[0])
      ? ' '
      : '';
  final tail = after.isNotEmpty && !_isSpace(after[0]) && !_closes(after[0])
      ? ' '
      : '';
  final inserted = '$lead$words$tail';
  final cursor = before.length + inserted.length;
  return (
    value: TextEditingValue(
      text: '$before$inserted$after',
      selection: TextSelection.collapsed(offset: cursor),
    ),
    from: before.length,
    to: cursor,
  );
}

bool _isSpace(String ch) => ch.trim().isEmpty;

/// Punctuation that attaches to the word before it: no space goes in front of it.
bool _closes(String ch) => '.,;:!?)'.contains(ch);
