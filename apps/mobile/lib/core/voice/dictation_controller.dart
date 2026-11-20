/// One dictation: the states the composer shows, the 5-minute cap, and the promises that hold in every one of
/// them: the microphone is released when the session ends, the text said so far is never thrown away by the
/// app (only by the person's X), and nothing is ever sent.
library;

import 'dart:async';

import 'package:flutter/foundation.dart';

import 'package:atlas_mobile/core/voice/dictation.dart';

enum DictationPhase {
  /// Not dictating.
  idle,

  /// The microphone was asked for and the recognizer is starting (it may install a language pack).
  starting,

  /// The microphone is live; [DictationController.text] is the transcript so far, shown dimmed.
  listening,

  /// The microphone is off (the cap, the app went to the background, the engine ended or failed) and the text is
  /// kept, waiting for the person's check or X.
  review,
}

/// What happened, for the screen reader ([DictationController.notes]).
enum DictationNote { started, stopped, discarded, capReached, backgrounded }

/// How long [DictationController.finish] waits for the recognizer's last words before it keeps what it has.
const _settle = Duration(seconds: 3);

class DictationController extends ChangeNotifier {
  DictationController(this._recognizer, {this.cap = dictationCap});

  final DictationRecognizer _recognizer;

  /// How long a session may run before it stops and keeps what was said.
  final Duration cap;

  DictationPhase _phase = DictationPhase.idle;
  String _text = '';
  DictationEngine? _engine;
  DictationFailure? _failure;
  bool _nothingHeard = false;

  StreamSubscription<DictationEvent>? _sub;
  Timer? _capTimer;
  Completer<void>? _ended;

  /// The recognizer was asked to finish: its [DictationFinal] is the answer, not the engine ending on its own.
  bool _stopping = false;
  int _generation = 0;
  final _notes = StreamController<DictationNote>.broadcast(sync: true);

  DictationPhase get phase => _phase;

  /// The transcript so far (the whole session's, not a delta).
  String get text => _text;

  /// The engine of the running or last session; what the screen names next to the mic.
  DictationEngine? get engine => _engine;

  /// Why the last session failed, until the next one starts.
  DictationFailure? get failure => _failure;

  /// The last session ended with no words, without failing.
  bool get nothingHeard => _nothingHeard;

  bool get active => _phase != DictationPhase.idle;

  Stream<DictationNote> get notes => _notes.stream;

  /// Starts a session on [engine]. [DictationException] from the recognizer ends in [failure], never a throw.
  Future<void> begin({
    required String localeTag,
    required DictationEngine engine,
  }) async {
    if (_phase != DictationPhase.idle) return;
    final gen = ++_generation;
    _stopping = false;
    _set(
      DictationPhase.starting,
      engine: engine,
      text: '',
      failure: null,
      nothing: false,
    );
    _sub = _recognizer.events.listen(_onEvent);
    _ended = Completer<void>();
    try {
      await _recognizer.start(
        localeTag: localeTag,
        online: engine == DictationEngine.online,
      );
    } on DictationException catch (e) {
      if (gen != _generation) return;
      await _drop();
      _set(DictationPhase.idle, failure: e.code);
      return;
    }
    if (gen != _generation) {
      // Discarded or sent to the background while the recognizer started: it must not keep the microphone.
      await _recognizer.cancel();
      return;
    }
    _set(DictationPhase.listening);
    _capTimer = Timer(cap, _capReached);
    _notes.add(DictationNote.started);
  }

  /// The check while listening: ends the session, waits for the last words, and returns the text (empty: nothing
  /// was said, [nothingHeard] says so). The controller is idle afterwards.
  Future<String> finish() async {
    if (_phase != DictationPhase.listening) return take();
    final gen = _generation;
    await _settleStop();
    if (gen != _generation) return '';
    _notes.add(DictationNote.stopped);
    return take();
  }

  /// The check on a session that already ended ([DictationPhase.review]): the text, and idle again.
  String take() {
    final out = _text.trim();
    _generation++;
    _nothingHeard = out.isEmpty && _phase != DictationPhase.idle;
    _set(DictationPhase.idle, text: '');
    return out;
  }

  /// The X: drops everything, releases the microphone, leaves no event behind.
  Future<void> discard() async {
    if (_phase == DictationPhase.idle) return;
    _generation++;
    await _drop();
    _set(DictationPhase.idle, text: '', failure: null, nothing: false);
    _notes.add(DictationNote.discarded);
  }

  /// The app left the foreground: the microphone is released AT ONCE (no waiting for the recognizer) and what was
  /// said so far is kept for review.
  Future<void> background() async {
    switch (_phase) {
      case DictationPhase.idle || DictationPhase.review:
        return;
      case DictationPhase.starting:
        _generation++;
        await _drop();
        _set(DictationPhase.idle, text: '');
        return;
      case DictationPhase.listening:
        _generation++;
        await _drop();
        _ending(DictationNote.backgrounded);
    }
  }

  void _capReached() async {
    if (_phase != DictationPhase.listening) return;
    final gen = _generation;
    await _settleStop();
    if (gen != _generation) return;
    _ending(DictationNote.capReached);
  }

  /// Asks the recognizer to finish and waits (bounded) for its final text; the microphone is released after.
  Future<void> _settleStop() async {
    _stopping = true;
    _capTimer?.cancel();
    final ended = _ended;
    try {
      await _recognizer.stop();
      await ended?.future.timeout(_settle);
    } on TimeoutException {
      // The last partial is what there is.
    } on Object catch (e) {
      debugPrint('atlas-mobile: dictation stop failed: $e');
    }
    await _drop();
  }

  /// The session is over, mic off, with [_text]: review it, or idle when there is nothing to review.
  void _ending(DictationNote note) {
    _notes.add(note);
    if (_text.trim().isEmpty) {
      _set(DictationPhase.idle, text: '', nothing: true);
    } else {
      _set(DictationPhase.review);
    }
  }

  void _onEvent(DictationEvent e) {
    if (_phase == DictationPhase.idle || _phase == DictationPhase.review) {
      return;
    }
    switch (e) {
      case DictationPartial(:final text):
        _set(_phase, text: text);
      case DictationFinal(:final text):
        if (text.trim().isNotEmpty) _text = text;
        _ended?.complete();
        // The engine ended on its own (not asked for): the text waits for the check or the X.
        if (!_stopping && _phase == DictationPhase.listening) {
          final gen = _generation;
          _drop().then((_) {
            if (gen == _generation) _ending(DictationNote.stopped);
          });
        }
      case DictationFailureEvent(:final code):
        _ended?.complete();
        final gen = _generation;
        _drop().then((_) {
          if (gen != _generation) return;
          _failure = code;
          if (_text.trim().isEmpty) {
            _set(DictationPhase.idle, text: '');
          } else {
            _set(DictationPhase.review);
          }
        });
    }
  }

  /// Releases the microphone and every listener. Safe to call twice.
  Future<void> _drop() async {
    _capTimer?.cancel();
    _capTimer = null;
    final sub = _sub;
    _sub = null;
    // Not awaited: a broadcast stream's cancel has nothing to wait for, and nothing here depends on it.
    unawaited(sub?.cancel());
    if (!(_ended?.isCompleted ?? true)) _ended?.complete();
    _ended = null;
    await _recognizer.cancel();
  }

  void _set(
    DictationPhase phase, {
    DictationEngine? engine,
    String? text,
    Object? failure = _keep,
    bool? nothing,
  }) {
    _phase = phase;
    if (engine != null) _engine = engine;
    if (text != null) _text = text;
    if (!identical(failure, _keep)) _failure = failure as DictationFailure?;
    if (nothing != null) _nothingHeard = nothing;
    notifyListeners();
  }

  static const Object _keep = Object();

  @override
  void dispose() {
    _generation++;
    _capTimer?.cancel();
    _sub?.cancel();
    // Leaving the chat releases the microphone; nothing is awaited in dispose.
    unawaited(_recognizer.cancel());
    _notes.close();
    super.dispose();
  }
}
