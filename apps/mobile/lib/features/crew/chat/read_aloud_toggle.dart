/// The two pieces of "read replies aloud" in a crewmate's chat: the switch (this phone only, off by default) and
/// the "Speaking" indicator. The indicator is a speaker, never the red dot: that one means a microphone is live
/// (`dictation_bar.dart`), and the two are never the same thing.
library;

import 'package:flutter/services.dart';
import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/l10n.dart';
import 'package:atlas_mobile/core/voice/speech.dart';
import 'package:atlas_mobile/features/crew/chat/glass.dart';
import 'package:atlas_mobile/features/crew/chat_motion.dart';

/// The switch in the chat header: a speaker, struck through while it is off. A frosted round button like the
/// others, announced as a switch with its state.
class ReadAloudButton extends StatelessWidget {
  const ReadAloudButton({super.key, required this.on, required this.onChanged});

  final bool on;
  final ValueChanged<bool> onChanged;

  @override
  Widget build(BuildContext context) {
    final l = context.l10n;
    final c = AtlasColors.of(context);
    return Semantics(
      container: true,
      toggled: on,
      label: l.mobile_voice_read_aloud,
      hint: l.mobile_voice_read_aloud_hint,
      excludeSemantics: true,
      onTap: () => onChanged(!on),
      child: Pressable(
        onTap: () {
          HapticFeedback.selectionClick();
          onChanged(!on);
        },
        child: SizedBox.square(
          dimension: 48,
          child: Center(
            child: GlassPill(
              circle: true,
              child: SizedBox.square(
                dimension: 44,
                child: Icon(
                  on ? Icons.volume_up_rounded : Icons.volume_off_rounded,
                  size: 18,
                  color: on ? c.accent : c.muted,
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

/// "Speaking" with a way to stop it, shown while the phone reads a reply, and the one line it leaves when it
/// could not. Nothing at all otherwise.
class SpeakingIndicator extends StatelessWidget {
  const SpeakingIndicator({super.key, required this.speech});

  final SpeechController speech;

  @override
  Widget build(BuildContext context) => ListenableBuilder(
    listenable: speech,
    builder: (context, _) {
      final c = AtlasColors.of(context);
      final l = context.l10n;
      final small = Theme.of(context).textTheme.bodySmall!;
      if (speech.speaking) {
        return Padding(
          padding: const EdgeInsets.only(bottom: Space.sm),
          child: Semantics(
            button: true,
            liveRegion: true,
            label: l.mobile_voice_speaking_stop,
            excludeSemantics: true,
            onTap: speech.stop,
            child: Pressable(
              onTap: speech.stop,
              child: ConstrainedBox(
                constraints: const BoxConstraints(minHeight: 48),
                child: Center(
                  widthFactor: 1,
                  child: GlassPill(
                    padding: const EdgeInsets.symmetric(
                      horizontal: Space.lg,
                      vertical: Space.sm,
                    ),
                    child: Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        Icon(
                          Icons.volume_up_rounded,
                          size: 16,
                          color: c.accent,
                        ),
                        const SizedBox(width: Space.sm),
                        Text(
                          l.mobile_voice_speaking,
                          style: small.copyWith(color: c.text),
                        ),
                        const SizedBox(width: Space.md),
                        Icon(Icons.stop_rounded, size: 16, color: c.muted),
                      ],
                    ),
                  ),
                ),
              ),
            ),
          ),
        );
      }
      if (speech.failed) {
        return Padding(
          padding: const EdgeInsets.only(bottom: Space.sm),
          child: Semantics(
            liveRegion: true,
            child: Text(
              l.mobile_voice_speech_failed,
              style: small.copyWith(color: c.error),
            ),
          ),
        );
      }
      return const SizedBox.shrink();
    },
  );
}
