/// The chat's composer: a floating pill whose send dot buds out of its right end once there is text.
library;

import 'dart:async';

import 'package:material_ui/material_ui.dart';
import 'package:flutter/semantics.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/l10n.dart';
import 'package:atlas_mobile/core/voice/dictation.dart';
import 'package:atlas_mobile/core/voice/dictation_controller.dart';
import 'package:atlas_mobile/core/voice/platform_dictation.dart';
import 'package:atlas_mobile/core/voice/platform_speech.dart';
import 'package:atlas_mobile/core/voice/speech.dart';
import 'package:atlas_mobile/features/crew/chat/attachments.dart';
import 'package:atlas_mobile/features/crew/chat/dictation_bar.dart';
import 'package:atlas_mobile/features/crew/chat/dictation_flow.dart';
import 'package:atlas_mobile/features/crew/chat/glass.dart';
import 'package:atlas_mobile/features/crew/chat/image_source.dart';
import 'package:atlas_mobile/features/crew/chat_motion.dart';

// ── Composer: a floating pill, like the desktop's ────────────────────────────

class ChatComposer extends ConsumerStatefulWidget {
  const ChatComposer({
    super.key,
    required this.controller,
    required this.sending,
    required this.online,
    required this.queued,
    required this.onSend,
    this.voice = false,
  });

  final TextEditingController controller;
  final bool sending;
  final bool online;
  final int queued;

  /// Whether the crew allows dictation (`crew_voice_status.enabled`): the mic is there only then.
  final bool voice;

  /// Sends the text with the images picked so far; [dictated] is true when any of it was dictated (hostd tells
  /// the agent to expect homophones). `true`: it went, and the images are let go; `false`: it failed, and they
  /// stay for the next try.
  final Future<bool> Function(List<PendingImage> images, bool dictated) onSend;

  @override
  ConsumerState<ChatComposer> createState() => _ChatComposerState();
}

class _ChatComposerState extends ConsumerState<ChatComposer>
    with WidgetsBindingObserver {
  final List<PendingImage> _images = [];
  String? _error;

  late final DictationController _dictation = DictationController(
    ref.read(dictationProvider),
  );
  StreamSubscription<DictationNote>? _notes;

  /// The app's voice, kept: `ref` must not be read once the widget is going away.
  late final SpeechController _speech;

  /// The message as it was when dictation started: the words are drawn into it, and the X puts it back.
  TextEditingValue _draft = TextEditingValue.empty;

  /// Any of the message was dictated; hostd gets `dictated: true` with the task.
  bool _dictated = false;

  /// The tap on the mic is being answered (support, permissions, opt-in): a second tap does nothing.
  bool _preparing = false;
  DictationFailure? _shownFailure;
  bool _shownNothing = false;

  /// The announcement for a screen reader that has no announce event (the live region says it instead).
  String _live = '';

  /// The app left the foreground while dictating: said when it comes back, a screen reader cannot hear it before.
  bool _backgroundedWhileListening = false;

  @override
  void initState() {
    super.initState();
    _speech = ref.read(speechControllerProvider);
    WidgetsBinding.instance.addObserver(this);
    _dictation.addListener(_onDictation);
    _notes = _dictation.notes.listen(_onNote);
    controller.addListener(_forgetDictated);
  }

  @override
  void didUpdateWidget(ChatComposer old) {
    super.didUpdateWidget(old);
    if (old.controller != widget.controller) {
      old.controller.removeListener(_forgetDictated);
      widget.controller.addListener(_forgetDictated);
    }
    // The crew switched voice off while the mic was live: it must not keep listening to a hidden control.
    if (old.voice && !widget.voice) _dictation.discard();
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    controller.removeListener(_forgetDictated);
    _notes?.cancel();
    _dictation.removeListener(_onDictation);
    _dictation.dispose();
    _speech.releaseMic(this);
    super.dispose();
  }

  /// An empty message has nothing dictated in it.
  void _forgetDictated() {
    if (_dictated && controller.text.isEmpty && !_dictation.active) {
      _dictated = false;
    }
  }

  /// The microphone is released the moment the app is not on screen (`inactive` is only a system prompt or the
  /// control center: it keeps going); what was said waits for the check or the X.
  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    switch (state) {
      case AppLifecycleState.hidden ||
          AppLifecycleState.paused ||
          AppLifecycleState.detached:
        _dictation.background();
      case AppLifecycleState.resumed:
        if (_backgroundedWhileListening) {
          _backgroundedWhileListening = false;
          _say(context.l10n.mobile_voice_announce_background);
        }
      case AppLifecycleState.inactive:
        break;
    }
  }

  /// What the session reports, said to a screen reader; failures and "nothing heard" also show as the line above
  /// the field.
  void _onDictation() {
    final l = context.l10n;
    final failure = _dictation.failure;
    final nothing = _dictation.nothingHeard;
    if (failure != null && failure != _shownFailure) {
      _error = dictationFailureText(l, failure);
    } else if (nothing && !_shownNothing) {
      _error = l.mobile_voice_nothing_heard;
    }
    _shownFailure = failure;
    _shownNothing = nothing;
    // The microphone is closed for good: the voice may read again.
    if (!_dictation.active && !_preparing) {
      _speech.releaseMic(this);
    }
    if (mounted) setState(() {});
  }

  void _onNote(DictationNote note) {
    final l = context.l10n;
    switch (note) {
      case DictationNote.started:
        _say(l.mobile_voice_announce_started);
      case DictationNote.stopped:
        _say(l.mobile_voice_announce_stopped);
      case DictationNote.discarded:
        _say(l.mobile_voice_announce_discarded);
      case DictationNote.capReached:
        _say(l.mobile_voice_announce_cap);
      case DictationNote.backgrounded:
        _backgroundedWhileListening = true;
    }
  }

  void _say(String message) {
    if (!mounted) return;
    if (MediaQuery.maybeSupportsAnnounceOf(context) ?? false) {
      SemanticsService.sendAnnouncement(
        View.of(context),
        message,
        Directionality.of(context),
      );
    } else {
      setState(() => _live = message);
    }
  }

  /// The mic: what this phone can do for the app's language, the permissions in context, then the session. The
  /// voice that reads replies goes quiet at once, on the tap, and stays quiet while the microphone is open.
  Future<void> _dictate() async {
    if (_preparing || _dictation.active || widget.sending) return;
    final speech = _speech;
    if (!speech.claimMic(this)) return;
    _preparing = true;
    try {
      final plan = await prepareDictation(context, ref);
      if (plan == null || !mounted) return;
      setState(() => _error = null);
      _draft = controller.value;
      await _dictation.begin(localeTag: plan.tag, engine: plan.engine);
    } finally {
      _preparing = false;
      if (!_dictation.active) speech.releaseMic(this);
    }
  }

  /// The check: ends the recognizer if it still listens, then puts the words at the cursor (or over the
  /// selection) of the message as it was. Never sends.
  Future<void> _insertDictation() async {
    final words = _dictation.phase == DictationPhase.listening
        ? await _dictation.finish()
        : _dictation.take();
    if (!mounted || words.isEmpty) return;
    controller.value = insertDictation(_draft, words);
    setState(() => _dictated = true);
    _say(context.l10n.mobile_voice_announce_inserted);
  }

  /// The X: the message goes back as it was and nothing of the words stays.
  Future<void> _discardDictation() async {
    await _dictation.discard();
    if (!mounted) return;
    controller.value = _draft;
  }

  TextEditingController get controller => widget.controller;
  bool get sending => widget.sending;
  bool get online => widget.online;
  int get queued => widget.queued;

  void _fail(String message) => setState(() => _error = message);

  Future<void> _chooseSource() async {
    final source = await askPhotoSource(context);
    if (source == null || !mounted) return;
    List<PickedImage>? picked;
    try {
      picked = await pickPhotos(context, ref, source);
    } on Object catch (e) {
      // No camera, or a picker that failed: say so, never a silent nothing.
      if (mounted) _fail('Could not open the ${source.name}: $e');
      return;
    }
    final got = picked;
    if (!mounted || got == null) return;
    setState(() => _error = takePicked(_images, got));
  }

  Future<void> _send() async {
    final images = List<PendingImage>.of(_images);
    final ok = await widget.onSend(images, _dictated);
    if (ok && mounted) {
      setState(() {
        _images.clear();
        _error = null;
        _dictated = false;
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return SafeArea(
      top: false,
      child: Padding(
        padding: const EdgeInsets.fromLTRB(
          Space.md,
          Space.xs,
          Space.md,
          Space.sm,
        ),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            if (queued > 0)
              Padding(
                padding: const EdgeInsets.only(
                  left: Space.lg,
                  bottom: Space.sm,
                ),
                child: Text(
                  queued == 1 ? '1 task queued' : '$queued tasks queued',
                  style: Theme.of(context).textTheme.bodySmall!
                      .copyWith(color: c.muted),
                ),
              ),
            if (_error != null)
              Padding(
                padding: const EdgeInsets.only(
                  left: Space.lg,
                  bottom: Space.sm,
                ),
                child: Text(
                  _error!,
                  style: Theme.of(context).textTheme.bodySmall!
                      .copyWith(color: c.error),
                ),
              ),
            if (_images.isNotEmpty)
              Padding(
                padding: const EdgeInsets.only(bottom: Space.sm),
                child: SizedBox(
                  height: 64,
                  child: ListView.separated(
                    scrollDirection: Axis.horizontal,
                    padding: const EdgeInsets.symmetric(horizontal: 4),
                    itemCount: _images.length,
                    separatorBuilder: (_, _) => const SizedBox(width: Space.sm),
                    itemBuilder: (_, i) {
                      final image = _images[i];
                      return _PendingThumb(
                        key: ObjectKey(image),
                        image: image,
                        onRemove: () => setState(() => _images.remove(image)),
                      );
                    },
                  ),
                ),
              ),
            // For a screen reader with no announce event: the last thing dictation said, as a live region.
            if (_live.isNotEmpty)
              Semantics(
                liveRegion: true,
                label: _live,
                child: const SizedBox.shrink(),
              ),
            if (_dictation.active)
              DictationBar(
                controller: _dictation,
                draft: _draft,
                onInsert: _insertDictation,
                onDiscard: _discardDictation,
              )
            // The field keeps its shape; once there is text, the send dot detaches from
            // its right end and floats beside it (_SendSlot).
            else
              ValueListenableBuilder<TextEditingValue>(
                valueListenable: controller,
                builder: (_, value, _) {
                  final ready = value.text.trim().isNotEmpty && !sending;
                  return Row(
                    crossAxisAlignment: CrossAxisAlignment.end,
                    children: [
                      // Attach: the camera or the photo library; the picked images wait above the field.
                      Padding(
                        padding: const EdgeInsets.only(right: Space.sm),
                        child: GlassRoundButton(
                          icon: Icons.add_photo_alternate_outlined,
                          label: 'Attach image',
                          onTap: _images.length >= maxAttachments
                              ? () => _fail(
                                  'A task takes at most $maxAttachments images',
                                )
                              : _chooseSource,
                        ),
                      ),
                      Expanded(
                        child: GlassPill(
                          child: TextField(
                            controller: controller,
                            minLines: 1,
                            maxLines: 6,
                            cursorColor: c.text,
                            textCapitalization: TextCapitalization.sentences,
                            style: Theme.of(context).textTheme.titleMedium!
                                .copyWith(
                                  color: c.text,
                                  height: 1.35,
                                  fontWeight: FontWeight.w400,
                                ),
                            decoration: InputDecoration(
                              hintText: online
                                  ? 'Message'
                                  : 'Offline — queued when back',
                              hintStyle: Theme.of(context)
                                  .textTheme
                                  .titleMedium!
                                  .copyWith(
                                    color: c.faint,
                                    fontWeight: FontWeight.w400,
                                  ),
                              border: InputBorder.none,
                              isCollapsed: true,
                              // The app theme gives every field a filled box; the pill is it.
                              filled: false,
                              enabledBorder: InputBorder.none,
                              focusedBorder: InputBorder.none,
                              contentPadding: const EdgeInsets.fromLTRB(
                                22,
                                15,
                                22,
                                15,
                              ),
                            ),
                          ),
                        ),
                      ),
                      if (widget.voice)
                        Padding(
                          padding: const EdgeInsets.only(left: Space.sm),
                          child: GlassRoundButton(
                            icon: Icons.mic_none_rounded,
                            label: context.l10n.mobile_voice_dictate,
                            onTap: _dictate,
                          ),
                        ),
                      _SendSlot(
                        shown: ready || sending,
                        fieldColor: c.overlay.withValues(alpha: .82),
                        child: Semantics(
                          button: true,
                          label: 'Send',
                          child: Pressable(
                            onTap: ready
                                ? () {
                                    HapticFeedback.lightImpact();
                                    _send();
                                  }
                                : null,
                            child: Container(
                              width: _SendSlot.dot,
                              height: _SendSlot.dot,
                              decoration: BoxDecoration(
                                color: c.accent,
                                shape: BoxShape.circle,
                              ),
                              alignment: Alignment.center,
                              child: sending
                                  ? SizedBox.square(
                                      dimension: 18,
                                      child: CircularProgressIndicator(
                                        strokeWidth: 2,
                                        color: c.onAccent,
                                      ),
                                    )
                                  : Icon(
                                      Icons.arrow_upward_rounded,
                                      size: 22,
                                      color: c.onAccent,
                                    ),
                            ),
                          ),
                        ),
                      ),
                    ],
                  );
                },
              ),
          ],
        ),
      ),
    );
  }
}

/// One picked image above the field: a small square with a remove button.
class _PendingThumb extends StatelessWidget {
  const _PendingThumb({super.key, required this.image, required this.onRemove});
  final PendingImage image;
  final VoidCallback onRemove;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return SizedBox.square(
      dimension: 64,
      child: Stack(
        children: [
          ClipRRect(
            borderRadius: BorderRadius.circular(12),
            child: Image.memory(
              image.bytes,
              width: 64,
              height: 64,
              fit: BoxFit.cover,
              // A truncated file passes the signature check; it must not break the composer.
              errorBuilder: (_, _, _) => Container(
                width: 64,
                height: 64,
                color: c.overlay,
                child: Icon(Icons.broken_image_outlined, color: c.faint),
              ),
            ),
          ),
          // Inside the thumbnail, never poking out of the list: a badge half outside its slot lost half its
          // taps. A 32 px target around the 24 px dot.
          Positioned(
            top: 0,
            right: 0,
            child: Semantics(
              container: true,
              button: true,
              label: 'Remove ${image.name}',
              child: Pressable(
                onTap: onRemove,
                child: SizedBox.square(
                  dimension: 32,
                  child: Center(
                    child: Container(
                      width: 24,
                      height: 24,
                      decoration: BoxDecoration(
                        color: c.overlay,
                        shape: BoxShape.circle,
                        border: Border.all(color: c.border),
                      ),
                      child: Icon(Icons.close_rounded, size: 14, color: c.text),
                    ),
                  ),
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }
}

/// Where the send dot lives. Empty field: no slot, the field has the whole row. Text in: the
/// dot buds out of the field's right end, stretches away on a thinning neck that snaps, and
/// springs into place while the field gives up the room. Back to empty: pulled back in.
class _SendSlot extends StatefulWidget {
  const _SendSlot({
    required this.shown,
    required this.fieldColor,
    required this.child,
  });
  final bool shown;
  final Color fieldColor;
  final Widget child;

  static const double dot = 46;
  static const double gap = 8;

  @override
  State<_SendSlot> createState() => _SendSlotState();
}

class _SendSlotState extends State<_SendSlot>
    with SingleTickerProviderStateMixin {
  late final AnimationController _c = AnimationController(
    vsync: this,
    duration: Motion.slow * 2,
    reverseDuration: Motion.slow,
    value: widget.shown ? 1 : 0,
  );

  @override
  void didUpdateWidget(_SendSlot old) {
    super.didUpdateWidget(old);
    if (old.shown == widget.shown) return;
    if (Motion.reduced(context)) {
      _c.value = widget.shown ? 1 : 0;
    } else if (widget.shown) {
      _c.forward();
    } else {
      _c.reverse();
    }
  }

  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    const size = _SendSlot.dot, gap = _SendSlot.gap;
    return AnimatedBuilder(
      animation: _c,
      child: widget.child,
      builder: (_, child) {
        final t = _c.value;
        if (t == 0) return const SizedBox.shrink();
        // The field gives up the room smoothly while the dot travels out of it.
        final room = Curves.easeOutCubic.transform(t) * (size + gap);
        final travel = Curves.easeOutBack.transform(t);
        // The dot is never small or faded: it is a piece of the field's right end, full
        // size from the start, that pulls away to the right and snaps free.
        final dx = -(size + gap) * (1 - travel);
        // Its colour turns from the field's to the accent once it is out.
        final tint = Curves.easeIn.transform(((t - .2) / .5).clamp(0.0, 1.0));
        // The neck: full height at first (still one piece), pinching to nothing.
        final neck = (1 - t / .55).clamp(0.0, 1.0);
        final neckH = size * Curves.easeIn.transform(neck);
        return SizedBox(
          width: room,
          height: size,
          child: OverflowBox(
            alignment: Alignment.centerRight,
            minWidth: size + gap,
            maxWidth: size + gap,
            child: Stack(
              clipBehavior: Clip.none,
              alignment: Alignment.centerRight,
              children: [
                if (neckH > 0.5)
                  Positioned(
                    // From inside the field's end to the dot's centre.
                    left: -size * .5,
                    right: size / 2 - dx,
                    top: (size - neckH) / 2,
                    height: neckH,
                    child: ClipRRect(
                      borderRadius: BorderRadius.circular(neckH / 2),
                      child: ColoredBox(color: widget.fieldColor),
                    ),
                  ),
                Transform.translate(
                  offset: Offset(dx, 0),
                  child: ColorFiltered(
                    colorFilter: ColorFilter.mode(
                      widget.fieldColor.withValues(alpha: 1 - tint),
                      BlendMode.srcATop,
                    ),
                    child: child,
                  ),
                ),
              ],
            ),
          ),
        );
      },
    );
  }
}
