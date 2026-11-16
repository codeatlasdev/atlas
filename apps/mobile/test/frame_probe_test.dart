// The frame probe must see what a screen recording sees: these controls prove it catches a cross-fade, a
// fade-in of content that was already there and a layout shift, and stays quiet when there is none.
import 'package:material_ui/material_ui.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/frame_probe.dart';

Widget _app(Widget child) => MaterialApp(
  home: Scaffold(body: SafeArea(child: child)),
);

class _Switch extends StatefulWidget {
  const _Switch({required this.crossFade});
  final bool crossFade;
  @override
  State<_Switch> createState() => _SwitchState();
}

class _SwitchState extends State<_Switch> {
  int i = 0;
  @override
  Widget build(BuildContext context) => Column(
    children: [
      TextButton(
        onPressed: () => setState(() => i = 1),
        child: const Text('go'),
      ),
      Expanded(
        child: Stack(
          children: [
            for (final n in [0, 1])
              widget.crossFade
                  ? AnimatedOpacity(
                      opacity: i == n ? 1 : 0,
                      duration: const Duration(milliseconds: 260),
                      child: SizedBox.expand(key: ValueKey('page$n')),
                    )
                  : Opacity(
                      opacity: i == n ? 1 : 0,
                      child: SizedBox.expand(key: ValueKey('page$n')),
                    ),
          ],
        ),
      ),
    ],
  );
}

void main() {
  final pages = {
    'a': find.byKey(const ValueKey('page0')),
    'b': find.byKey(const ValueKey('page1')),
  };

  testWidgets('a cross-fade shows both pages at once: the probe catches it', (
    t,
  ) async {
    await t.pumpWidget(_app(const _Switch(crossFade: true)));
    final probe = FrameProbe(t, pages);
    await t.tap(find.text('go'));
    await probe.record(const Duration(milliseconds: 400));
    expect(
      () => probe.expectNeverBothVisible('a', 'b'),
      throwsA(isA<TestFailure>()),
    );
  });

  testWidgets('a cut never shows both pages: the probe stays quiet', (t) async {
    await t.pumpWidget(_app(const _Switch(crossFade: false)));
    final probe = FrameProbe(t, pages);
    await t.tap(find.text('go'));
    await probe.record(const Duration(milliseconds: 400));
    probe.expectNeverBothVisible('a', 'b');
  });

  testWidgets('content that fades in is caught; opaque content passes', (
    t,
  ) async {
    await t.pumpWidget(
      _app(
        Column(
          children: [
            TweenAnimationBuilder<double>(
              tween: Tween(begin: 0, end: 1),
              duration: const Duration(milliseconds: 200),
              builder: (_, v, child) => Opacity(opacity: v, child: child),
              child: const SizedBox(
                key: ValueKey('fading'),
                height: 20,
                width: 20,
              ),
            ),
            const SizedBox(key: ValueKey('solid'), height: 20, width: 20),
          ],
        ),
      ),
    );
    // The fade started on the frame pumpWidget drew: the first recorded frames catch it mid-way.
    final probe = FrameProbe(t, {
      'fading': find.byKey(const ValueKey('fading')),
      'solid': find.byKey(const ValueKey('solid')),
    });
    await probe.record(const Duration(milliseconds: 300));
    expect(
      () => probe.expectOpaqueWhenPresent('fading'),
      throwsA(isA<TestFailure>()),
    );
    probe.expectOpaqueWhenPresent('solid');
  });

  testWidgets(
    'a widget pushed down by late content is caught; a still one passes',
    (t) async {
      final late = ValueNotifier(false);
      await t.pumpWidget(
        _app(
          ValueListenableBuilder<bool>(
            valueListenable: late,
            builder: (_, show, _) => Column(
              children: [
                if (show) const SizedBox(height: 40),
                const SizedBox(key: ValueKey('row'), height: 20, width: 20),
              ],
            ),
          ),
        ),
      );
      final probe = FrameProbe(t, {'row': find.byKey(const ValueKey('row'))});
      Future<void>.delayed(
        const Duration(milliseconds: 50),
        () => late.value = true,
      );
      await probe.record(const Duration(milliseconds: 120));
      expect(() => probe.expectNoShift('row'), throwsA(isA<TestFailure>()));

      final still = FrameProbe(t, {'row': find.byKey(const ValueKey('row'))});
      await still.record(const Duration(milliseconds: 120));
      still.expectNoShift('row');
    },
  );
}
