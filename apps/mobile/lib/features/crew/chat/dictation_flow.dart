/// What happens between the tap on the mic and the first word: the permissions (asked in context, explained
/// first, a refusal is an explained state with a way to Settings) and, on a phone with no on-device recognizer
/// for the language, the labelled opt-in to the system recognizer.
library;

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/permissions/permission_ui.dart';
import 'package:atlas_mobile/core/permissions/permissions.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/l10n.dart';
import 'package:atlas_mobile/core/voice/dictation.dart';
import 'package:atlas_mobile/core/voice/platform_dictation.dart';

/// The name of an engine as the composer shows it, all the time a session runs.
String dictationEngineLabel(AppLocalizations l, DictationEngine e) =>
    switch (e) {
      DictationEngine.apple => l.mobile_voice_engine_apple,
      DictationEngine.android => l.mobile_voice_engine_android,
      DictationEngine.online => l.mobile_voice_engine_online,
    };

/// What a failed session says, with what to do next.
String dictationFailureText(AppLocalizations l, DictationFailure f) =>
    switch (f) {
      DictationFailure.permission => l.mobile_voice_error_permission,
      DictationFailure.busy => l.mobile_voice_error_busy,
      DictationFailure.interrupted => l.mobile_voice_error_interrupted,
      DictationFailure.language => l.mobile_voice_error_language,
      DictationFailure.failed => l.mobile_voice_error_failed,
    };

/// What a session will run on: the speech language and the engine.
typedef DictationPlan = ({String tag, DictationEngine engine});

/// Everything between a tap on a mic and the first word, for the composer and for a voice answer alike: what this
/// phone can do for the app's language, the labelled opt-in when it has no on-device recognizer, and the
/// permissions. Null: nothing starts (the person said not now, or a refusal was explained).
Future<DictationPlan?> prepareDictation(
  BuildContext context,
  WidgetRef ref,
) async {
  final tag = dictationLocaleTag(Localizations.localeOf(context));
  final support = await ref.read(dictationProvider).support(tag);
  if (!context.mounted) return null;
  var engine = support.onDevice;
  if (engine == null) {
    // Never a silent fallback to a network recognizer: it is the person's decision, with its label.
    if (!await askOnlineRecognizer(context, support) || !context.mounted) {
      return null;
    }
    engine = DictationEngine.online;
  }
  if (!await ensureDictationPermissions(
        context,
        ref,
        speech: support.needsSpeechPermission,
      ) ||
      !context.mounted) {
    return null;
  }
  return (tag: tag, engine: engine);
}

/// The microphone, then (iOS recognizers other than `SpeechAnalyzer`) speech recognition. `true`: both are
/// allowed, start. Otherwise nothing starts: a "not now" or a refusal says nothing more; a refusal that stays
/// ([PermissionState.blocked] and the like) opens the explained off state with Open Settings.
Future<bool> ensureDictationPermissions(
  BuildContext context,
  WidgetRef ref, {
  required bool speech,
}) async {
  for (final p in [
    AtlasPermission.microphone,
    if (speech) AtlasPermission.speech,
  ]) {
    if (!context.mounted) return false;
    final state = await askPermission(context, ref, p);
    if (state == PermissionState.granted) continue;
    if (state != PermissionState.notAsked && context.mounted) {
      await showPermissionOff<void>(context, p, state);
    }
    return false;
  }
  return true;
}

/// The phone has no on-device recognizer for the app's language. Says so, and offers the system recognizer only as
/// a labelled opt-in: it may send the audio to Apple or Google. `true` = the person chose it. A phone with no
/// recognizer at all gets the explanation and nothing to choose.
Future<bool> askOnlineRecognizer(
  BuildContext context,
  DictationSupport support,
) async {
  final chose = await showModalBottomSheet<bool>(
    context: context,
    isScrollControlled: true,
    showDragHandle: true,
    backgroundColor: AtlasColors.of(context).overlay,
    builder: (ctx) => SafeArea(
      child: SingleChildScrollView(
        padding: const EdgeInsets.fromLTRB(Space.xl, 0, Space.xl, Space.lg),
        child: _OnlineOptIn(support: support),
      ),
    ),
  );
  return chose == true;
}

class _OnlineOptIn extends StatelessWidget {
  const _OnlineOptIn({required this.support});
  final DictationSupport support;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final t = Theme.of(context).textTheme;
    final l = context.l10n;
    final body = !support.online
        ? l.mobile_voice_none_body
        : support.downloading
        ? l.mobile_voice_downloading_body
        : l.mobile_voice_unsupported_body;
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Center(
          child: Container(
            width: 64,
            height: 64,
            decoration: BoxDecoration(
              shape: BoxShape.circle,
              color: c.warning.withValues(alpha: .14),
            ),
            child: Icon(Icons.mic_off_outlined, size: 30, color: c.warning),
          ),
        ),
        const SizedBox(height: Space.lg),
        Text(
          l.mobile_voice_unsupported_title,
          style: t.titleMedium,
          textAlign: TextAlign.center,
        ),
        const SizedBox(height: Space.sm),
        Text(
          body,
          style: t.bodyLarge!.copyWith(color: c.muted, height: 1.4),
          textAlign: TextAlign.center,
        ),
        const SizedBox(height: Space.xl),
        if (support.online) ...[
          OutlinedButton(
            onPressed: () => Navigator.of(context).pop(true),
            child: Text(l.mobile_voice_use_online),
          ),
          const SizedBox(height: Space.xs),
          Text(
            l.mobile_voice_online_warning,
            style: t.bodySmall!.copyWith(color: c.warning),
            textAlign: TextAlign.center,
          ),
        ],
        const SizedBox(height: Space.xs),
        TextButton(
          onPressed: () => Navigator.of(context).pop(false),
          child: Text(l.common_action_close),
        ),
      ],
    );
  }
}
