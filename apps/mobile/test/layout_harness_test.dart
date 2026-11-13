import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/layout_harness.dart';

/// The harness is only worth having if it can fail: these screens are broken on
/// purpose, and `expectNoLayoutProblems` must say so.
void main() {
  Future<String> failureOf(
    WidgetTester t,
    Widget Function(BuildContext) screen, {
    bool tapTargets = true,
  }) async {
    try {
      await expectNoLayoutProblems(
        t,
        screen,
        tapTargets: tapTargets,
        settleFor: const Duration(milliseconds: 100),
      );
    } on TestFailure catch (e) {
      return e.message ?? '';
    }
    return '';
  }

  testWidgets('catches a Row with text that cannot shrink', (t) async {
    final msg = await failureOf(
      t,
      (_) => const Scaffold(
        body: Row(
          children: [
            Text('matheus-de-jesus-macbook-pro-16-inch-m4-pro-office'),
          ],
        ),
      ),
    );
    expect(msg, contains('overflowed'));
    // Each size and scale is named, so a failure says where it breaks.
    expect(msg, contains('320x568'));
  });

  testWidgets('catches content taller than the screen', (t) async {
    final msg = await failureOf(
      t,
      (_) => Scaffold(
        body: Column(
          children: [for (var i = 0; i < 40; i++) const Text('row')],
        ),
      ),
    );
    expect(msg, contains('overflowed'));
  });

  testWidgets('catches a tap target under 48 dp', (t) async {
    final msg = await failureOf(
      t,
      (_) => Scaffold(
        body: Center(
          child: GestureDetector(
            onTap: () {},
            child: Semantics(
              label: 'tiny',
              button: true,
              child: const SizedBox(width: 20, height: 20),
            ),
          ),
        ),
      ),
    );
    expect(msg, contains('tap target'));
  });

  testWidgets('catches an error thrown while building', (t) async {
    final msg = await failureOf(
      t,
      (_) => Scaffold(body: Builder(builder: (_) => throw StateError('boom'))),
      tapTargets: false,
    );
    expect(msg, contains('boom'));
  });

  testWidgets('passes a screen that wraps, scrolls and ellipsizes', (t) async {
    await expectNoLayoutProblems(
      t,
      (_) => Scaffold(
        body: ListView(
          children: [
            Row(
              children: const [
                Expanded(
                  child: Text(
                    'matheus-de-jesus-macbook-pro-16-inch-m4-pro-office',
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
              ],
            ),
            for (var i = 0; i < 40; i++) const Text('row'),
            TextButton(onPressed: () {}, child: const Text('ok')),
          ],
        ),
      ),
      settleFor: const Duration(milliseconds: 100),
    );
  });
}
