// AsyncView is the one rendering of a screen's async states: loading, error + "Try again", empty, offline,
// protocol mismatch, data. A golden pins the whole set so a change to any of them is seen.
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/connection/require_mux.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/protocol/protocol_version.dart';
import 'package:atlas_mobile/core/ui/async_view.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';

const _device = MacDevice(nodeId: 'mac-1', name: 'Studio');

Widget _view(
  AsyncValue<List<String>> v, {
  VoidCallback? onRetry,
  bool withEmpty = true,
}) => ProviderScope(
  child: MaterialApp(
    theme: atlasTheme(),
    home: Scaffold(
      body: AsyncView<List<String>>(
        value: v,
        errorTitle: "Couldn't read the list",
        onRetry: onRetry,
        isEmpty: withEmpty ? (l) => l.isEmpty : null,
        emptyTitle: 'Nothing yet',
        emptyMessage: 'Things you add show here.',
        data: (l) => ListView(children: [for (final s in l) Text(s)]),
      ),
    ),
  ),
);

void main() {
  testWidgets('loading: the one spinner, no data, no empty copy', (t) async {
    await t.pumpWidget(_view(const AsyncLoading()));
    expect(find.byType(CircularProgressIndicator), findsOneWidget);
    expect(find.text('Nothing yet'), findsNothing);
  });

  testWidgets('data: the builder, not the empty state', (t) async {
    await t.pumpWidget(_view(const AsyncData(['a', 'b'])));
    expect(find.text('a'), findsOneWidget);
    expect(find.text('b'), findsOneWidget);
    expect(find.text('Nothing yet'), findsNothing);
  });

  testWidgets('empty data: the empty copy, not the builder', (t) async {
    await t.pumpWidget(_view(const AsyncData(<String>[])));
    expect(find.text('Nothing yet'), findsOneWidget);
    expect(find.text('Things you add show here.'), findsOneWidget);
  });

  testWidgets('without isEmpty an empty list still goes to the builder', (
    t,
  ) async {
    await t.pumpWidget(_view(const AsyncData(<String>[]), withEmpty: false));
    expect(find.text('Nothing yet'), findsNothing);
  });

  testWidgets('error: its text and "Try again" that calls the retry', (
    t,
  ) async {
    var retried = 0;
    await t.pumpWidget(
      _view(
        AsyncError(StateError('the server refused'), StackTrace.empty),
        onRetry: () => retried++,
      ),
    );
    expect(find.text("Couldn't read the list"), findsOneWidget);
    expect(find.text('the server refused'), findsOneWidget);
    await t.tap(find.text('Try again'));
    expect(retried, 1);
  });

  testWidgets('error without a retry shows no button', (t) async {
    await t.pumpWidget(_view(AsyncError(Exception('boom'), StackTrace.empty)));
    expect(find.text('Try again'), findsNothing);
    expect(find.text('Exception: boom'), findsOneWidget);
  });

  testWidgets('offline: the shared "can\'t reach" state with Try again', (
    t,
  ) async {
    await t.pumpWidget(
      _view(AsyncError(const DeviceOffline(_device), StackTrace.empty)),
    );
    expect(find.text("Can't reach Studio"), findsOneWidget);
    expect(find.text('Try again'), findsOneWidget);
    expect(find.text("Couldn't read the list"), findsNothing);
  });

  testWidgets('protocol mismatch: the update message, never a retry', (
    t,
  ) async {
    const m = ProtocolMismatch(
      deviceName: 'Studio',
      hostProto: 1,
      appProto: kAppProto,
      hostIsOlder: true,
    );
    await t.pumpWidget(
      _view(
        AsyncError(const DeviceProtocolMismatch(_device, m), StackTrace.empty),
        onRetry: () {},
      ),
    );
    expect(find.text('Update Atlas on Studio'), findsOneWidget);
    expect(find.textContaining('atlas update'), findsOneWidget);
    expect(find.text('Try again'), findsNothing);
  });

  testWidgets('golden: every state of the set', (t) async {
    await t.binding.setSurfaceSize(const Size(393, 1400));
    const m = ProtocolMismatch(
      deviceName: 'Studio',
      hostProto: 1,
      appProto: kAppProto,
      hostIsOlder: true,
    );
    final states = <String, AsyncValue<List<String>>>{
      'loading': const AsyncLoading(),
      'empty': const AsyncData(<String>[]),
      'error': AsyncError(StateError('the server refused'), StackTrace.empty),
      'offline': AsyncError(const DeviceOffline(_device), StackTrace.empty),
      'mismatch': AsyncError(
        const DeviceProtocolMismatch(_device, m),
        StackTrace.empty,
      ),
    };
    await t.pumpWidget(
      ProviderScope(
        child: MaterialApp(
          theme: atlasTheme(),
          home: Scaffold(
            body: Column(
              children: [
                for (final e in states.entries)
                  Expanded(
                    child: AsyncView<List<String>>(
                      value: e.value,
                      errorTitle: "Couldn't read the list",
                      onRetry: () {},
                      isEmpty: (l) => l.isEmpty,
                      emptyTitle: 'Nothing yet',
                      emptyMessage: 'Things you add show here.',
                      data: (l) => const SizedBox(),
                    ),
                  ),
              ],
            ),
          ),
        ),
      ),
    );
    await expectLater(
      find.byType(MaterialApp),
      matchesGoldenFile('goldens/async_view_states.png'),
    );
  });
}
