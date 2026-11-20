/// Answering a card by voice: a tool approval or a choice. The words are recognised on the phone (the same
/// dictation as the composer, same on-device promise), and ONLY the transcript goes to hostd, which decides what
/// it means and whether voice may answer this call at all. The app never maps a word to an allow: it shows
/// the verdict hostd sends back.
///
/// - decided (allow / deny / choice): hostd already answered; a snackbar says what happened.
/// - comment: the words are not a plain answer, nothing was decided; the screen hands them to the person to
///   confirm by tap ([VoiceAnswer.onComment]).
/// - needs tap: why voice cannot answer this one (Bypass, a rail, a destructive call, an untrusted source).
/// - unclear: nothing was heard that means an answer.
library;

import 'dart:async';

import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/protocol/voice_verdict.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/l10n.dart';
import 'package:atlas_mobile/core/voice/dictation_controller.dart';
import 'package:atlas_mobile/core/voice/platform_dictation.dart';
import 'package:atlas_mobile/core/voice/platform_speech.dart';
import 'package:atlas_mobile/features/crew/chat/dictation_bar.dart';
import 'package:atlas_mobile/features/crew/chat/dictation_flow.dart';
import 'package:atlas_mobile/features/crew/permissions/mode_text.dart';

/// Sends [transcript] (and the language, `en` or `pt`) to hostd and returns its verdict.
typedef VoiceAnswerer = Future<VoiceVerdict> Function(
  String transcript,
  String lang,
);

/// What the card asks, for the words that tell the person what to say.
enum VoiceCardKind { approval, choice }

class VoiceAnswer extends ConsumerStatefulWidget {
  const VoiceAnswer({
    super.key,
    required this.kind,
    required this.answer,
    this.onComment,
  });

  final VoiceCardKind kind;
  final VoiceAnswerer answer;

  /// A comment verdict: [text] is what was heard, nothing is decided. Null: the screen only says it.
  final void Function(String text)? onComment;

  @override
  ConsumerState<VoiceAnswer> createState() => _VoiceAnswerState();
}

class _VoiceAnswerState extends ConsumerState<VoiceAnswer>
    with WidgetsBindingObserver {
  late final DictationController _dictation = DictationController(
    ref.read(dictationProvider),
  );
  bool _preparing = false;
  bool _sending = false;

  /// One line under the button: what hostd said, or why nothing happened.
  String? _note;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    _dictation.addListener(_onDictation);
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _dictation.removeListener(_onDictation);
    _dictation.dispose();
    ref.read(speechControllerProvider).releaseMic(this);
    super.dispose();
  }

  /// The microphone is released at once when the app leaves the screen; what was said waits for the check.
  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.hidden ||
        state == AppLifecycleState.paused ||
        state == AppLifecycleState.detached) {
      _dictation.background();
    }
  }

  void _onDictation() {
    final l = context.l10n;
    final failure = _dictation.failure;
    if (failure != null && _note == null) {
      _note = dictationFailureText(l, failure);
    }
    if (!_dictation.active && !_preparing && !_sending) {
      ref.read(speechControllerProvider).releaseMic(this);
    }
    if (mounted) setState(() {});
  }

  Future<void> _start() async {
    if (_preparing || _dictation.active || _sending) return;
    final speech = ref.read(speechControllerProvider);
    if (!speech.claimMic(this)) {
      setState(() => _note = context.l10n.mobile_voice_answer_busy);
      return;
    }
    _preparing = true;
    setState(() => _note = null);
    try {
      final plan = await prepareDictation(context, ref);
      if (plan == null || !mounted) return;
      await _dictation.begin(localeTag: plan.tag, engine: plan.engine);
    } finally {
      _preparing = false;
      if (!_dictation.active) speech.releaseMic(this);
    }
  }

  Future<void> _discard() => _dictation.discard();

  /// The check: ends the recognizer, then sends what was heard and shows hostd's verdict.
  Future<void> _send() async {
    final words = _dictation.phase == DictationPhase.listening
        ? await _dictation.finish()
        : _dictation.take();
    if (!mounted) return;
    final l = context.l10n;
    if (words.isEmpty) {
      setState(() => _note = l.mobile_voice_nothing_heard);
      return;
    }
    final messenger = ScaffoldMessenger.maybeOf(context);
    final lang = voiceLang(Localizations.localeOf(context).languageCode);
    setState(() {
      _sending = true;
      _note = null;
    });
    try {
      final verdict = await widget.answer(words, lang);
      _tell(messenger, l, verdict);
      if (!mounted) return;
      setState(
        () => _note = verdict.kind.decided ? null : _noteFor(l, verdict),
      );
      if (verdict.kind == VoiceVerdictKind.comment) {
        widget.onComment?.call(verdict.comment ?? words);
      }
    } on Object catch (e) {
      if (mounted) setState(() => _note = approvalFailureText(l, e));
    } finally {
      if (mounted) {
        setState(() => _sending = false);
        ref.read(speechControllerProvider).releaseMic(this);
      }
    }
  }

  /// A decided answer goes to the snackbar: the card usually goes away with it.
  void _tell(
    ScaffoldMessengerState? messenger,
    AppLocalizations l,
    VoiceVerdict v,
  ) {
    if (!v.kind.decided) return;
    HapticFeedback.mediumImpact();
    messenger?.showSnackBar(SnackBar(content: Text(_decidedText(l, v))));
  }

  String _decidedText(AppLocalizations l, VoiceVerdict v) => switch (v.kind) {
    VoiceVerdictKind.allow => l.mobile_voice_answer_allowed,
    VoiceVerdictKind.deny => l.mobile_voice_answer_denied,
    _ => l.mobile_voice_answer_chose(v.choice ?? ''),
  };

  String _noteFor(AppLocalizations l, VoiceVerdict v) => switch (v.kind) {
    VoiceVerdictKind.comment => l.mobile_voice_answer_comment(v.comment ?? ''),
    VoiceVerdictKind.needsTap => switch (v.reason) {
      VoiceRefusal.bypass => l.mobile_voice_answer_tap_bypass,
      VoiceRefusal.rail => l.mobile_voice_answer_tap_rail,
      VoiceRefusal.destructive => l.mobile_voice_answer_tap_destructive,
      VoiceRefusal.untrusted => l.mobile_voice_answer_tap_untrusted,
      null => l.mobile_voice_answer_tap_other,
    },
    _ =>
      widget.kind == VoiceCardKind.approval
          ? l.mobile_voice_answer_unclear_approval
          : l.mobile_voice_answer_unclear_choice,
  };

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final l = context.l10n;
    final small = Theme.of(context).textTheme.bodySmall!;
    if (_dictation.active) {
      return DictationBar(
        controller: _dictation,
        draft: TextEditingValue.empty,
        insertLabel: l.mobile_voice_answer_send,
        onInsert: _send,
        onDiscard: _discard,
      );
    }
    if (_sending) {
      return Semantics(
        liveRegion: true,
        label: l.mobile_voice_answer_sending,
        excludeSemantics: true,
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            SizedBox.square(
              dimension: 14,
              child: CircularProgressIndicator(strokeWidth: 2, color: c.muted),
            ),
            const SizedBox(width: Space.sm),
            Text(
              l.mobile_voice_answer_sending,
              style: small.copyWith(color: c.muted),
            ),
          ],
        ),
      );
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.end,
      mainAxisSize: MainAxisSize.min,
      children: [
        Semantics(
          button: true,
          label: l.mobile_voice_answer_button,
          hint: widget.kind == VoiceCardKind.approval
              ? l.mobile_voice_answer_hint_approval
              : l.mobile_voice_answer_hint_choice,
          excludeSemantics: true,
          onTap: _start,
          child: TextButton.icon(
            onPressed: _start,
            icon: const Icon(Icons.mic_none_rounded, size: 18),
            label: Text(l.mobile_voice_answer_button),
          ),
        ),
        if (_note != null)
          Semantics(
            liveRegion: true,
            child: Padding(
              padding: const EdgeInsets.only(top: Space.xs),
              child: Text(
                _note!,
                textAlign: TextAlign.end,
                style: small.copyWith(color: c.muted, height: 1.35),
              ),
            ),
          ),
      ],
    );
  }
}
