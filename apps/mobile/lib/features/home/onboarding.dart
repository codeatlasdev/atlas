import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import 'package:atlas_mobile/core/demo/demo_transport.dart';
import 'package:atlas_mobile/core/ui/atlas_logo.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/home/home_screen.dart';
import 'package:atlas_mobile/features/home/onboarding_art.dart';

/// First run (no Mac paired yet): three short pages, then how to pair.
class Onboarding extends StatefulWidget {
  const Onboarding({super.key, this.paired = false});

  /// A computer is already paired (debug: onboarding shown anyway): offer to skip pairing.
  final bool paired;

  @override
  State<Onboarding> createState() => _OnboardingState();
}

class _Page {
  const _Page(this.title, this.body, this.art);
  final String title;
  final String body;
  final Widget Function(BuildContext) art;
}

class _OnboardingState extends State<Onboarding> {
  final _pages = PageController();
  int _i = 0;

  late final _content = <_Page>[
    _Page(
      'Your computer, in your pocket',
      'Projects, services, agents and terminals from Atlas Desktop, live on your phone.',
      (_) => const AtlasLogo(size: 148, animated: true),
    ),
    _Page(
      'No account. Straight to your computer.',
      'Your phone talks to your computer directly, end-to-end encrypted. When a network blocks that, '
          'an encrypted relay carries the traffic and cannot read it.',
      (_) => const SecureLinkArt(),
    ),
    _Page(
      'Keep your agents moving',
      'See what each agent is doing, read the conversation, send the next prompt '
          'and answer permission requests from anywhere.',
      (_) => const AgentStoryArt(),
    ),
  ];

  bool get _last => _i == _content.length;

  @override
  void dispose() {
    _pages.dispose();
    super.dispose();
  }

  void _finish() {
    final c = ProviderScope.containerOf(context);
    c.read(onboardingSeenProvider.notifier).state = true;
  }

  void _next() => _pages.nextPage(duration: Motion.slow, curve: Motion.ease);

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return SafeArea(
      child: Column(
        children: [
          Align(
            alignment: Alignment.topRight,
            child: AnimatedOpacity(
              opacity: _last ? 0 : 1,
              duration: Motion.normal,
              child: TextButton(
                onPressed: _last
                    ? null
                    : () => _pages.animateToPage(
                        _content.length,
                        duration: Motion.slow,
                        curve: Motion.ease,
                      ),
                child: Text('Skip', style: TextStyle(color: c.muted)),
              ),
            ),
          ),
          Expanded(
            child: PageView(
              controller: _pages,
              onPageChanged: (i) => setState(() => _i = i),
              children: [
                for (var i = 0; i < _content.length; i++)
                  _Parallax(
                    controller: _pages,
                    index: i,
                    art: _content[i].art(context),
                    child: _PageText(page: _content[i]),
                  ),
                _Parallax(
                  controller: _pages,
                  index: _content.length,
                  art: const ScanArt(),
                  child: _HowToPair(active: _last),
                ),
              ],
            ),
          ),
          Padding(
            padding: const EdgeInsets.fromLTRB(Space.xl, 0, Space.xl, Space.lg),
            child: Column(
              children: [
                _Dots(count: _content.length + 1, index: _i),
                const SizedBox(height: Space.xl),
                SizedBox(
                  width: double.infinity,
                  child: FilledButton(
                    onPressed: _last
                        ? () =>
                              widget.paired ? _finish() : context.push('/pair')
                        : _next,
                    child: AnimatedSwitcher(
                      duration: Motion.normal,
                      child: Text(
                        _last
                            ? (widget.paired
                                  ? 'Open my computer'
                                  : 'Scan pairing code')
                            : 'Continue',
                        key: ValueKey(_last),
                      ),
                    ),
                  ),
                ),
                const SizedBox(height: Space.sm),
                TextButton(
                  onPressed: openPrivacy,
                  child: Text(
                    'Privacy',
                    style: TextStyle(color: c.faint, fontSize: 12),
                  ),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

/// One onboarding page: the illustration and the text move at different speeds while
/// swiping (the art travels further and fades), so pages feel layered, not slid.
class _Parallax extends StatelessWidget {
  const _Parallax({
    required this.controller,
    required this.index,
    required this.art,
    required this.child,
  });

  final PageController controller;
  final int index;
  final Widget art;
  final Widget child;

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: controller,
    builder: (context, _) {
      final page = controller.hasClients && controller.position.haveDimensions
          ? controller.page ?? 0
          : controller.initialPage.toDouble();
      // -1 (one page left) .. 0 (centered) .. 1 (one page right)
      final d = (index - page).clamp(-1.0, 1.0);
      final reduced = Motion.reduced(context);
      // Short screens / large text: the page scales down as a whole instead of overflowing.
      return LayoutBuilder(
        builder: (context, box) {
          final w = box.maxWidth;
          return Padding(
            padding: const EdgeInsets.symmetric(horizontal: Space.xl),
            child: Center(
              child: FittedBox(
                fit: BoxFit.scaleDown,
                child: SizedBox(
                  width: w - Space.xl * 2,
                  child: Column(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Transform.translate(
                        offset: Offset(reduced ? 0 : d * w * .35, 0),
                        child: Opacity(
                          opacity: (1 - d.abs() * 1.4).clamp(0.0, 1.0),
                          child: Transform.scale(
                            scale: reduced ? 1 : 1 - d.abs() * .12,
                            child: SizedBox(
                              height: 180,
                              child: Center(child: art),
                            ),
                          ),
                        ),
                      ),
                      const SizedBox(height: Space.xxl),
                      Transform.translate(
                        offset: Offset(reduced ? 0 : -d * w * .12, 0),
                        child: Opacity(
                          opacity: (1 - d.abs() * 1.2).clamp(0.0, 1.0),
                          child: child,
                        ),
                      ),
                    ],
                  ),
                ),
              ),
            ),
          );
        },
      );
    },
  );
}

class _PageText extends StatelessWidget {
  const _PageText({required this.page});
  final _Page page;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Column(
      children: [
        Text(
          page.title,
          textAlign: TextAlign.center,
          style: Theme.of(context).textTheme.headlineSmall
              ?.copyWith(fontWeight: FontWeight.w700, letterSpacing: -.4),
        ),
        const SizedBox(height: Space.md),
        Text(
          page.body,
          textAlign: TextAlign.center,
          style: TextStyle(color: c.muted, fontSize: 16, height: 1.45),
        ),
      ],
    );
  }
}

/// Slides and fades in when [active] turns on, [index] steps after the first.
class _StaggerIn extends StatelessWidget {
  const _StaggerIn({
    required this.active,
    required this.index,
    required this.child,
  });
  final bool active;
  final int index;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    const step = 110, base = 260;
    final total = base + step * index;
    return TweenAnimationBuilder<double>(
      tween: Tween(end: active || Motion.reduced(context) ? 1 : 0),
      duration: Duration(milliseconds: total),
      curve: Interval(step * index / total, 1, curve: Motion.ease),
      builder: (_, t, child) => Opacity(
        opacity: t,
        child: Transform.translate(
          offset: Offset(0, 14 * (1 - t)),
          child: child,
        ),
      ),
      child: child,
    );
  }
}

/// The last page: where the pairing code lives on the Mac.
class _HowToPair extends StatelessWidget {
  const _HowToPair({required this.active});

  /// The page is in view: its steps come in one after another.
  final bool active;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    Widget step(int n, String text, String? detail) => Padding(
      padding: const EdgeInsets.only(bottom: Space.lg),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Container(
            width: 28,
            height: 28,
            alignment: Alignment.center,
            decoration: BoxDecoration(
              color: c.accent.withValues(alpha: .15),
              shape: BoxShape.circle,
            ),
            child: Text(
              '$n',
              style: TextStyle(color: c.accent, fontWeight: FontWeight.w700),
            ),
          ),
          const SizedBox(width: Space.md),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                const SizedBox(height: 4),
                Text(
                  text,
                  style: const TextStyle(
                    fontSize: 16,
                    fontWeight: FontWeight.w600,
                  ),
                ),
                if (detail != null) ...[
                  const SizedBox(height: 2),
                  Text(detail, style: TextStyle(color: c.muted, height: 1.4)),
                ],
              ],
            ),
          ),
        ],
      ),
    );
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          'Pair your computer',
          style: Theme.of(context).textTheme.headlineSmall
              ?.copyWith(fontWeight: FontWeight.w700, letterSpacing: -.4),
        ),
        const SizedBox(height: Space.xl),
        _StaggerIn(
          active: active,
          index: 0,
          child: step(1, 'Open Atlas Desktop on your Mac or Linux PC', null),
        ),
        _StaggerIn(
          active: active,
          index: 1,
          child: step(
            2,
            'Settings → Integrations → Mobile → Pair a phone',
            'A code appears for two minutes.',
          ),
        ),
        _StaggerIn(
          active: active,
          index: 2,
          child: step(
            3,
            'Scan it with this phone',
            'Your computer must stay awake to be reachable.',
          ),
        ),
      ],
    );
  }
}

class _Dots extends StatelessWidget {
  const _Dots({required this.count, required this.index});
  final int count;
  final int index;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Row(
      mainAxisAlignment: MainAxisAlignment.center,
      children: [
        for (var i = 0; i < count; i++)
          AnimatedContainer(
            duration: Motion.normal,
            curve: Motion.ease,
            margin: const EdgeInsets.symmetric(horizontal: 3),
            width: i == index ? 20 : 6,
            height: 6,
            decoration: BoxDecoration(
              color: i == index ? c.accent : c.border,
              borderRadius: BorderRadius.circular(3),
            ),
          ),
      ],
    );
  }
}
