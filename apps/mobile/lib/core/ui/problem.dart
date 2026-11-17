import 'package:material_ui/material_ui.dart';
import 'package:flutter/services.dart';

import 'package:atlas_mobile/core/connection/require_mux.dart';

import 'atlas_theme.dart';

/// What went wrong, said for a person: a title, one plain sentence, the steps that fix it, and the raw
/// text only under "Details". Every error state in the app is built from one of these ([Problem.of]), so
/// the same failure always reads the same way.
class Problem {
  const Problem({
    required this.title,
    required this.message,
    this.steps = const [],
    this.details,
    this.icon = Icons.error_outline_rounded,
    this.tone = ProblemTone.error,
  });

  final String title;
  final String message;

  /// What to do, in order. Text in backticks is shown as a command.
  final List<String> steps;

  /// The raw error, for "Details" (copyable). Null when there is nothing more than [message].
  final String? details;
  final IconData icon;
  final ProblemTone tone;

  /// The problem behind [error], seen while doing [doing] ("read your inbox"), on [computer] when known.
  factory Problem.of(Object error, {required String doing, String? computer}) {
    final where = computer ?? 'your computer';
    final raw = errorText(error);
    if (error is DeviceOffline) {
      return Problem(
        icon: Icons.wifi_off_rounded,
        tone: ProblemTone.warning,
        title: "Can't reach $where",
        message: 'The phone could not open a connection to it.',
        steps: [
          'Check that $where is on and awake (not asleep with the lid closed).',
          'Open Atlas Desktop there, or run `atlas hostd` in a terminal.',
          'The phone needs internet or the same Wi-Fi; a VPN on either side can block it.',
        ],
      );
    }
    if (error is DeviceProtocolMismatch) {
      return Problem(
        icon: Icons.system_update_alt_rounded,
        tone: ProblemTone.warning,
        title: error.mismatch.title,
        message: error.mismatch.message,
      );
    }
    final unknown = RegExp(r'unknown variant `([a-z_]+)`').firstMatch(raw);
    if (unknown != null) {
      return Problem(
        icon: Icons.system_update_alt_rounded,
        tone: ProblemTone.warning,
        title: 'Atlas on $where is out of date',
        message:
            'This app asked for something ("${unknown.group(1)}") that the Atlas installed there '
            'does not have yet.',
        steps: [
          'On $where, run `atlas update` (or update Atlas Desktop from its menu).',
          'Come back here and tap Try again.',
        ],
        details: raw,
      );
    }
    final lower = raw.toLowerCase();
    if (lower.contains('timed out') || lower.contains('timeout')) {
      return Problem(
        icon: Icons.hourglass_bottom_rounded,
        tone: ProblemTone.warning,
        title: '$where took too long to answer',
        message: "The request to $doing didn't come back in time.",
        steps: [
          'A slow or switching connection is the usual cause: try again in a moment.',
          'If it keeps happening, check that $where is not busy or asleep.',
        ],
        details: raw,
      );
    }
    if (lower.contains('not allowed') ||
        lower.contains('forbidden') ||
        lower.contains('scope')) {
      return Problem(
        icon: Icons.lock_outline_rounded,
        title: "This phone isn't allowed to do that",
        message: '$where refused to let this phone $doing.',
        steps: [
          'On $where, open Atlas Desktop → Settings → Mobile.',
          'Give this phone the permission it needs, then try again.',
        ],
        details: raw,
      );
    }
    return Problem(
      title: "Couldn't $doing",
      message: _firstSentence(raw),
      steps: const [
        'Try again. If it keeps failing, the details below say what the computer answered.',
      ],
      details: raw.length > 90 ? raw : null,
    );
  }

  static String _firstSentence(String s) {
    final t = s.trim();
    if (t.isEmpty) return 'Something went wrong.';
    final cut = t.indexOf(RegExp(r'[.:\n]'));
    final first = cut > 12 && cut < 90 ? t.substring(0, cut) : t;
    final one = first.length > 90 ? '${first.substring(0, 87)}…' : first;
    return one[0].toUpperCase() + one.substring(1);
  }
}

enum ProblemTone { error, warning }

/// A [Problem] as a full state: tinted icon, title, sentence, numbered steps, actions, collapsible details.
class ProblemView extends StatefulWidget {
  const ProblemView({
    super.key,
    required this.problem,
    this.onRetry,
    this.action,
  });

  final Problem problem;
  final VoidCallback? onRetry;

  /// Replaces "Try again" when the fix is elsewhere.
  final Widget? action;

  @override
  State<ProblemView> createState() => _ProblemViewState();
}

class _ProblemViewState extends State<ProblemView> {
  bool _open = false;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final p = widget.problem;
    final tone = p.tone == ProblemTone.warning ? c.warning : c.error;
    final t = Theme.of(context).textTheme;
    return LayoutBuilder(
      builder: (context, box) => SingleChildScrollView(
        padding: const EdgeInsets.fromLTRB(Space.xl, Space.xl, Space.xl, 140),
        child: ConstrainedBox(
          constraints: BoxConstraints(minHeight: box.maxHeight - 160),
          child: Column(
            mainAxisAlignment: MainAxisAlignment.center,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Container(
                width: 52,
                height: 52,
                decoration: BoxDecoration(
                  color: tone.withValues(alpha: .12),
                  borderRadius: BorderRadius.circular(16),
                  border: Border.all(color: tone.withValues(alpha: .28)),
                ),
                child: Icon(p.icon, color: tone, size: 26),
              ),
              const SizedBox(height: Space.lg),
              Text(p.title, style: t.titleLarge),
              const SizedBox(height: Space.sm),
              Text(
                p.message,
                style: t.bodyLarge!.copyWith(color: c.muted, height: 1.45),
              ),
              if (p.steps.isNotEmpty) ...[
                const SizedBox(height: Space.lg),
                Container(
                  width: double.infinity,
                  padding: const EdgeInsets.all(Space.md),
                  decoration: BoxDecoration(
                    color: c.base,
                    borderRadius: BorderRadius.circular(Space.radius),
                    border: Border.all(color: c.borderSubtle),
                  ),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      for (var i = 0; i < p.steps.length; i++)
                        Padding(
                          padding: EdgeInsets.only(
                            bottom: i == p.steps.length - 1 ? 0 : Space.md,
                          ),
                          child: Row(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Container(
                                width: 22,
                                height: 22,
                                alignment: Alignment.center,
                                decoration: BoxDecoration(
                                  color: c.raised,
                                  shape: BoxShape.circle,
                                ),
                                child: Text(
                                  '${i + 1}',
                                  style: t.labelSmall!.copyWith(
                                    color: c.text,
                                    fontWeight: FontWeight.w700,
                                  ),
                                ),
                              ),
                              const SizedBox(width: Space.md),
                              Expanded(child: _StepText(p.steps[i])),
                            ],
                          ),
                        ),
                    ],
                  ),
                ),
              ],
              const SizedBox(height: Space.lg),
              // Wraps: a long action label next to Details does not fit a 320 pt phone in one line.
              Wrap(
                spacing: Space.sm,
                runSpacing: Space.sm,
                crossAxisAlignment: WrapCrossAlignment.center,
                children: [
                  ?widget.action ??
                      (widget.onRetry == null
                          ? null
                          : FilledButton.icon(
                              onPressed: widget.onRetry,
                              icon: const Icon(Icons.refresh_rounded, size: 18),
                              label: const Text('Try again'),
                            )),
                  if (p.details != null)
                    TextButton(
                      onPressed: () => setState(() => _open = !_open),
                      child: Text(_open ? 'Hide details' : 'Details'),
                    ),
                ],
              ),
              if (p.details != null && _open) ...[
                const SizedBox(height: Space.sm),
                GestureDetector(
                  onLongPress: () {
                    Clipboard.setData(ClipboardData(text: p.details!));
                    ScaffoldMessenger.maybeOf(context)?.showSnackBar(
                      const SnackBar(content: Text('Details copied')),
                    );
                  },
                  child: Container(
                    width: double.infinity,
                    padding: const EdgeInsets.all(Space.md),
                    decoration: BoxDecoration(
                      color: c.ground,
                      borderRadius: BorderRadius.circular(Space.radiusSm),
                      border: Border.all(color: c.borderSubtle),
                    ),
                    child: SelectableText(
                      p.details!,
                      style: t.bodySmall!.copyWith(
                        fontFamily: 'Menlo',
                        color: c.muted,
                        height: 1.4,
                      ),
                    ),
                  ),
                ),
                const SizedBox(height: Space.xs),
                Text('Long-press to copy', style: t.labelSmall),
              ],
            ],
          ),
        ),
      ),
    );
  }
}

/// A step with `backtick` spans drawn as commands.
class _StepText extends StatelessWidget {
  const _StepText(this.text);
  final String text;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final base = Theme.of(context).textTheme.bodyMedium!
        .copyWith(color: c.text, height: 1.45);
    final parts = text.split('`');
    return Text.rich(
      TextSpan(
        style: base,
        children: [
          for (var i = 0; i < parts.length; i++)
            i.isOdd
                ? WidgetSpan(
                    alignment: PlaceholderAlignment.middle,
                    child: Container(
                      padding: const EdgeInsets.symmetric(
                        horizontal: 6,
                        vertical: 1,
                      ),
                      decoration: BoxDecoration(
                        color: c.raised,
                        borderRadius: BorderRadius.circular(5),
                      ),
                      child: Text(
                        parts[i],
                        style: base.copyWith(
                          fontFamily: 'Menlo',
                          fontSize: TextSize.small,
                          color: c.accent,
                        ),
                      ),
                    ),
                  )
                : TextSpan(text: parts[i]),
        ],
      ),
    );
  }
}
