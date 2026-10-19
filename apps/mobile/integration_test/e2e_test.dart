// End to end on an iOS Simulator against a real `atlas hostd` over iroh: pairing,
// projects, the project screen, an agent and its terminal. Run by
// `bun scripts/mobile-e2e.ts`, which starts an isolated hostd with a fixture
// project and an agent terminal and passes the pairing URL in `E2E_PAIR_URL`.
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

import 'package:atlas_mobile/features/home/home_screen.dart';
import 'package:atlas_mobile/features/pair/pair_screen.dart';
import 'package:atlas_mobile/main.dart';
import 'package:atlas_mobile/src/rust/frb_generated.dart';

const _pairUrl = String.fromEnvironment('E2E_PAIR_URL');

/// Pump until [f] finds something, or fail after [timeout]. Network and iroh
/// make every step asynchronous; fixed sleeps would be flaky or slow.
Future<void> _until(
  WidgetTester t,
  Finder f, {
  Duration timeout = const Duration(seconds: 30),
}) async {
  final end = DateTime.now().add(timeout);
  while (DateTime.now().isBefore(end)) {
    await t.pump(const Duration(milliseconds: 200));
    if (f.evaluate().isNotEmpty) return;
  }
  // What the screen showed instead: the first thing to read when a step fails.
  final shown = [
    for (final e in find.byType(RichText).evaluate())
      (e.widget as RichText).text.toPlainText(),
  ].where((t) => t.trim().isNotEmpty).join(' | ');
  fail('not found within ${timeout.inSeconds}s: $f\nscreen: $shown');
}

/// Wait for [f], let the page transition finish (a widget found while its route
/// still slides in is off screen), then tap it.
Future<void> _tap(WidgetTester t, Finder f) async {
  await _until(t, f);
  await t.pump(const Duration(milliseconds: 600));
  await t.tap(f.first);
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('pair, open the project, its agent and the terminal', (t) async {
    expect(_pairUrl, isNotEmpty, reason: 'run through scripts/mobile-e2e.ts');
    await RustLib.init();
    await t.pumpWidget(const ProviderScope(child: AtlasMobileApp()));
    await t.pump();
    final container = ProviderScope.containerOf(
      t.element(find.byType(AtlasMobileApp)),
    );

    // Pairing: the code a scan would read, through the same notifier, kept alive
    // the way the pairing screen keeps it (it watches the state).
    final keep = container.listen(pairNotifierProvider, (_, _) {});
    await container.read(pairNotifierProvider.notifier).pair(_pairUrl);
    expect(
      container.read(pairNotifierProvider).status,
      PairStatus.success,
      reason: '${container.read(pairNotifierProvider).error}',
    );
    keep.close();
    container.invalidate(pairedDevicesProvider);

    // Home (onboarding off: ATLAS_ALWAYS_ONBOARD=false): the computer's projects.
    await _tap(t, find.text('e2e-shop'));

    // Project screen: services from atlas.yaml; a service's sheet has its last log line.
    await _until(t, find.text('worker'));
    await _tap(t, find.text('api'));
    await _until(t, find.textContaining('listening on 4100'));
    await t.tapAt(const Offset(20, 80)); // outside the sheet: close it
    await t.pump(const Duration(milliseconds: 600));

    // Agents tab: the agent from the hook, with its last reply; then its chat.
    await _tap(t, find.textContaining('Agents'));
    await _tap(t, find.textContaining('e2e reply'));

    // Its terminal: the replay arrives (the bug of an empty terminal).
    await _tap(t, find.byTooltip('Terminal'));
    await _until(t, find.textContaining('hello from e2e', findRichText: true));
  });
}
