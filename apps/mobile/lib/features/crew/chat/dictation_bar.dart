/// The composer while a dictation runs or waits: X (discard), the words so far drawn where they would land (the
/// recognizer's partial text dimmed), the recognizer's name, a live-microphone indicator and the check (insert).
library;

import 'package:flutter/services.dart';
import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/l10n.dart';
import 'package:atlas_mobile/core/voice/dictation.dart';
import 'package:atlas_mobile/core/voice/dictation_controller.dart';
import 'package:atlas_mobile/features/crew/chat/dictation_flow.dart';
import 'package:atlas_mobile/features/crew/chat/glass.dart';
import 'package:atlas_mobile/features/crew/chat_motion.dart';

class DictationBar extends StatelessWidget {
  const DictationBar({
    super.key,
    required this.controller,
    required this.draft,
    required this.onInsert,
    required this.onDiscard,
  });

  final DictationController controller;

  /// The message as it was when dictation started: the words are drawn into it at its cursor or selection.
  final TextEditingValue draft;
  final VoidCallback onInsert;
  final VoidCallback onDiscard;

  static const double dot = 46;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final l = context.l10n;
    final phase = controller.phase;
    final live = phase == DictationPhase.listening;
    final canInsert =
        phase == DictationPhase.listening || phase == DictationPhase.review;
    final engine = controller.engine;
    final style = Theme.of(context).textTheme.titleMedium!
        .copyWith(color: c.text, height: 1.35, fontWeight: FontWeight.w400);
    final splice = spliceDictation(draft, controller.text);
    final text = splice.value.text;
    final dictated = splice.to > splice.from;
    return Row(
      crossAxisAlignment: CrossAxisAlignment.end,
      children: [
        Padding(
          padding: const EdgeInsets.only(right: Space.sm),
          child: GlassRoundButton(
            icon: Icons.close_rounded,
            label: l.mobile_voice_discard,
            onTap: onDiscard,
          ),
        ),
        Expanded(
          child: GlassPill(
            child: Padding(
              padding: const EdgeInsets.fromLTRB(22, 12, 22, 12),
              child: Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(
                    children: [
                      _Status(phase: phase),
                      const Spacer(),
                      if (engine != null)
                        Flexible(
                          child: Semantics(
                            label: l.mobile_voice_engine(
                              dictationEngineLabel(l, engine),
                            ),
                            excludeSemantics: true,
                            child: Text(
                              dictationEngineLabel(l, engine),
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: Theme.of(context).textTheme.bodySmall!
                                  .copyWith(
                                    color: engine.onDevice
                                        ? c.muted
                                        : c.warning,
                                  ),
                            ),
                          ),
                        ),
                    ],
                  ),
                  const SizedBox(height: Space.xs),
                  ConstrainedBox(
                    constraints: const BoxConstraints(maxHeight: 132),
                    child: SingleChildScrollView(
                      reverse: splice.to >= text.length,
                      child: Text.rich(
                        TextSpan(
                          style: style,
                          children: [
                            TextSpan(text: text.substring(0, splice.from)),
                            // What the recognizer has heard so far: not part of the message until the check.
                            TextSpan(
                              text: text.substring(splice.from, splice.to),
                              style: TextStyle(color: c.muted),
                            ),
                            TextSpan(text: text.substring(splice.to)),
                            if (!dictated && live && text.isEmpty)
                              TextSpan(
                                text: l.mobile_voice_placeholder,
                                style: TextStyle(color: c.faint),
                              ),
                          ],
                        ),
                      ),
                    ),
                  ),
                ],
              ),
            ),
          ),
        ),
        Padding(
          padding: const EdgeInsets.only(left: Space.sm),
          child: Semantics(
            button: true,
            enabled: canInsert,
            label: l.mobile_voice_insert,
            child: Pressable(
              onTap: canInsert
                  ? () {
                      HapticFeedback.lightImpact();
                      onInsert();
                    }
                  : null,
              child: Opacity(
                opacity: canInsert ? 1 : .4,
                child: Container(
                  width: dot,
                  height: dot,
                  decoration: BoxDecoration(
                    color: c.accent,
                    shape: BoxShape.circle,
                  ),
                  alignment: Alignment.center,
                  child: Icon(Icons.check_rounded, size: 24, color: c.onAccent),
                ),
              ),
            ),
          ),
        ),
      ],
    );
  }
}

/// The left of the status row: a red pulsing dot and "Listening" while the microphone is live, a spinner while it
/// starts, and "Microphone off" once it was released. The dot is the "mic is live" indicator.
class _Status extends StatelessWidget {
  const _Status({required this.phase});
  final DictationPhase phase;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final l = context.l10n;
    final small = Theme.of(context).textTheme.bodySmall!;
    return switch (phase) {
      DictationPhase.listening => Semantics(
        liveRegion: true,
        label: '${l.mobile_voice_listening}. ${l.mobile_voice_mic_live}',
        excludeSemantics: true,
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            const MicLiveDot(),
            const SizedBox(width: Space.sm),
            Text(
              l.mobile_voice_listening,
              style: small.copyWith(color: c.text),
            ),
          ],
        ),
      ),
      DictationPhase.starting => Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          SizedBox.square(
            dimension: 12,
            child: CircularProgressIndicator(strokeWidth: 2, color: c.muted),
          ),
          const SizedBox(width: Space.sm),
          Text(l.mobile_voice_starting, style: small.copyWith(color: c.muted)),
        ],
      ),
      _ => Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(Icons.mic_off_outlined, size: 14, color: c.muted),
          const SizedBox(width: Space.sm),
          Text(l.mobile_voice_mic_off, style: small.copyWith(color: c.muted)),
        ],
      ),
    };
  }
}

/// The red dot of a live microphone. It breathes, unless the person asked the phone for reduced motion: then it
/// is still, and still red.
class MicLiveDot extends StatefulWidget {
  const MicLiveDot({super.key});

  @override
  State<MicLiveDot> createState() => _MicLiveDotState();
}

class _MicLiveDotState extends State<MicLiveDot>
    with SingleTickerProviderStateMixin {
  late final AnimationController _c = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 900),
  );
  bool? _reduced;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final reduced = Motion.reduced(context);
    if (reduced == _reduced) return;
    _reduced = reduced;
    if (reduced) {
      _c
        ..stop()
        ..value = 1;
    } else {
      _c.repeat(reverse: true);
    }
  }

  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => FadeTransition(
    opacity: Tween<double>(begin: .35, end: 1).animate(_c),
    child: Container(
      width: 10,
      height: 10,
      decoration: BoxDecoration(
        color: AtlasColors.of(context).error,
        shape: BoxShape.circle,
      ),
    ),
  );
}
