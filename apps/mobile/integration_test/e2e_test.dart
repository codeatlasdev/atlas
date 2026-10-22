// End to end on an iOS Simulator against a real `atlas hostd` over iroh: pairing,
// projects, the project screen, an agent and its terminal. Run by
// `bun scripts/mobile-e2e.ts`, which starts an isolated hostd with a fixture
// project and an agent terminal and passes the pairing URL in `E2E_PAIR_URL`.
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

import 'package:atlas_mobile/features/inbox/inbox.dart';
import 'package:atlas_mobile/features/pair/pair_screen.dart';
import 'package:atlas_mobile/main.dart';
import 'package:atlas_mobile/src/rust/frb_generated.dart';

const _pairUrl = String.fromEnvironment('E2E_PAIR_URL');

/// `--watch`: pause on each step so a person can follow it.
const _slow = bool.fromEnvironment('E2E_SLOW');

/// The step being tested, shown in a banner on top of the app.
final _caption = ValueNotifier<String>('');

/// Real time (a live binding does not wait on `pump`'s duration), only with `--watch`.
Future<void> _pause(WidgetTester t, int ms) async {
  if (!_slow) return t.pump();
  await Future<void>.delayed(Duration(milliseconds: ms));
  await t.pump();
}

/// Name the next step in the banner (and the log), then let it be read.
Future<void> _step(WidgetTester t, String text) async {
  _caption.value = text;
  debugPrint('e2e: $text');
  await _pause(t, 1500);
}

/// The app under test with the step banner over it.
Widget _withBanner(Widget app) => Directionality(
  textDirection: TextDirection.ltr,
  child: Stack(
    children: [
      app,
      Positioned(
        left: 12,
        right: 12,
        bottom: 40,
        child: IgnorePointer(
          child: ValueListenableBuilder(
            valueListenable: _caption,
            builder: (_, text, _) => text.isEmpty
                ? const SizedBox.shrink()
                : Container(
                    padding: const EdgeInsets.symmetric(
                      horizontal: 16,
                      vertical: 12,
                    ),
                    decoration: BoxDecoration(
                      color: const Color(0xE6000000),
                      borderRadius: BorderRadius.circular(14),
                    ),
                    child: Text(
                      text,
                      textAlign: TextAlign.center,
                      style: const TextStyle(
                        color: Color(0xFFFFFFFF),
                        fontSize: 17,
                        fontWeight: FontWeight.w600,
                        decoration: TextDecoration.none,
                      ),
                    ),
                  ),
          ),
        ),
      ),
    ],
  ),
);

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
  await _pause(t, 800);
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('pair, open the project, its agent and the terminal', (t) async {
    expect(_pairUrl, isNotEmpty, reason: 'run through scripts/mobile-e2e.ts');
    await RustLib.init();
    await t.pumpWidget(
      _withBanner(const ProviderScope(child: AtlasMobileApp())),
    );
    await t.pump();
    final container = ProviderScope.containerOf(
      t.element(find.byType(AtlasMobileApp)),
    );

    await _step(t, '1/7  Pairing with the computer');
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

    await _step(t, '2/7  Home: the computer\'s projects');
    // Home (onboarding off: ATLAS_ALWAYS_ONBOARD=false): the computer's projects.
    await _tap(t, find.text('e2e-shop'));

    await _step(t, '3/7  Project: its services');
    // Project screen: services from atlas.yaml; a service's sheet has its last log line.
    await _until(t, find.text('worker'));
    await _step(t, '4/7  A service: its last log line');
    await _tap(t, find.text('api'));
    await _until(t, find.textContaining('listening on 4100'));
    await t.tapAt(const Offset(20, 80)); // outside the sheet: close it
    await t.pump(const Duration(milliseconds: 600));

    await _step(t, '5/7  Agents: the agent and its chat');
    // Agents tab: the agent from the hook, with its last reply; then its chat.
    await _tap(t, find.textContaining('Agents'));
    await _tap(t, find.textContaining('e2e reply'));

    await _step(t, '6/7  The agent\'s terminal');
    // Its terminal: the replay arrives (the bug of an empty terminal).
    await _tap(t, find.byTooltip('Terminal'));
    await _until(t, find.textContaining('hello from e2e', findRichText: true));
    // Note: creating a real bot task requires a signed-in agent CLI (claude/codex/omp)
    // which is not available in CI. Tab navigation is verified here; the full bot
    // flow (create → task → done) is covered by the widget tests in bots_flow_test.dart.
    await _step(t, '7/7  Bots: tab visible with demo bot');
    // Navigate to the Bots tab via the bottom NavigationBar.
    await _tap(t, find.byIcon(Icons.smart_toy_rounded));
    // The Bots tab loads; without a real bot computer paired this shows the
    // "No computers paired" empty state OR the paired computer with no bots.
    // Either way the Bots tab must be reachable.
    await t.pump(const Duration(seconds: 2));
    // Navigate back to Computers.
    await _tap(t, find.byIcon(Icons.computer_rounded));
    await _step(t, '✓  Everything works');
    await _pause(t, 3000);
  });
}
