import 'package:flutter/material.dart';
import 'package:go_router/go_router.dart';

import 'package:atlas_mobile/core/ui/atlas_logo.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';

/// First run (no Mac paired yet): three short pages, then how to pair.
class Onboarding extends StatefulWidget {
  const Onboarding({super.key});

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
      (c) => _Glyph(
        icon: Icons.lock_outline_rounded,
        color: AtlasColors.of(c).success,
      ),
    ),
    _Page(
      'Keep your agents moving',
      'See what each agent is doing, read the conversation, send the next prompt '
          'and answer permission requests from anywhere.',
      (c) => _Glyph(
        icon: Icons.auto_awesome_rounded,
        color: AtlasColors.of(c).accent,
      ),
    ),
  ];

  bool get _last => _i == _content.length;

  @override
  void dispose() {
    _pages.dispose();
    super.dispose();
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
                for (final p in _content) _PageView(page: p),
                const _HowToPair(),
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
                    onPressed: _last ? () => context.push('/pair') : _next,
                    child: AnimatedSwitcher(
                      duration: Motion.normal,
                      child: Text(
                        _last ? 'Scan pairing code' : 'Continue',
                        key: ValueKey(_last),
                      ),
                    ),
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

class _PageView extends StatelessWidget {
  const _PageView({required this.page});
  final _Page page;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: Space.xl),
      child: Column(
        mainAxisAlignment: MainAxisAlignment.center,
        children: [
          SizedBox(height: 160, child: Center(child: page.art(context))),
          const SizedBox(height: Space.xxl),
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
      ),
    );
  }
}

/// The last page: where the pairing code lives on the Mac.
class _HowToPair extends StatelessWidget {
  const _HowToPair();

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
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: Space.xl),
      child: Column(
        mainAxisAlignment: MainAxisAlignment.center,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const Center(child: _Glyph(icon: Icons.qr_code_scanner_rounded)),
          const SizedBox(height: Space.xxl),
          Text(
            'Pair your computer',
            style: Theme.of(context).textTheme.headlineSmall
                ?.copyWith(fontWeight: FontWeight.w700, letterSpacing: -.4),
          ),
          const SizedBox(height: Space.xl),
          step(1, 'Open Atlas Desktop on your Mac or Linux PC', null),
          step(
            2,
            'Settings → Mobile → Pair a phone',
            'A code appears for two minutes.',
          ),
          step(
            3,
            'Scan it with this phone',
            'Your computer must stay awake to be reachable.',
          ),
        ],
      ),
    );
  }
}

class _Glyph extends StatelessWidget {
  const _Glyph({required this.icon, this.color});
  final IconData icon;
  final Color? color;

  @override
  Widget build(BuildContext context) {
    final k = color ?? AtlasColors.of(context).accent;
    return Container(
      width: 112,
      height: 112,
      decoration: BoxDecoration(
        shape: BoxShape.circle,
        gradient: RadialGradient(
          colors: [k.withValues(alpha: .28), k.withValues(alpha: 0)],
        ),
      ),
      child: Icon(icon, size: 52, color: k),
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
