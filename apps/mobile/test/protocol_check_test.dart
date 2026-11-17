import 'dart:async';
import 'dart:convert';

import 'package:material_ui/material_ui.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'fake_transport.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/demo/demo_computer.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/protocol/protocol_version.dart';
import 'package:atlas_mobile/core/storage/mac_storage.dart';
import 'package:atlas_mobile/core/transport/transport.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/crew/list/crew_screen.dart';
import 'package:atlas_mobile/features/home/home_screen.dart';

import 'support/demo_harness.dart';

// Regression: #302. hostd's `info.proto` was never read, so a hostd older (or newer) than the app
// failed one command at a time with `bad request`. The check runs once per connection and shows ONE
// state instead.

Future<void> _settle(WidgetTester t) async {
  for (var i = 0; i < 30; i++) {
    await t.pump(const Duration(milliseconds: 100));
  }
}

class _Storage extends MacStorage {
  _Storage(this.devices) : super(storage: null);
  final List<MacDevice> devices;
  @override
  Future<List<MacDevice>> loadDevices() async => devices;
  @override
  Future<String?> loadSecretKey() async => 'ab';
}

class _FakeMacStorage extends MacStorage {
  _FakeMacStorage() : super(storage: null);
  @override
  Future<List<MacDevice>> loadDevices() async => [];
  @override
  Future<String?> loadSecretKey() async => null;
}

/// A hostd that answers `info` with [data] (the raw `data` object) and counts the dials.
class _Host {
  _Host(this.infoData);
  final Map<String, Object?>? infoData;
  int dials = 0;
  final transports = <_ReplyingTransport>[];

  Future<Transport> dial(MacDevice _, String _) async {
    dials++;
    final t = _ReplyingTransport(infoData);
    transports.add(t);
    return t;
  }
}

/// Answers every `info` request like hostd's server does: `{"id":N,"ok":true,"data":{...}}`.
final class _ReplyingTransport implements Transport {
  _ReplyingTransport(this.data);
  final Map<String, Object?>? data;
  final _fake = FakeTransport();

  List<String> get sentLines => _fake.sentLines;

  @override
  Stream<String> get incomingLines => _fake.incomingLines;

  @override
  Future<void> close() => _fake.close();

  @override
  Future<void> sendLine(String line) async {
    await _fake.sendLine(line);
    if (!line.contains('"cmd":"info"')) return;
    final id = jsonDecode(line)['id'];
    final reply = jsonEncode({'id': id, 'ok': true, 'data': data});
    scheduleMicrotask(() {
      try {
        _fake.injectLine(reply);
      } on StateError {
        // closed meanwhile
      }
    });
  }
}

/// The demo computer (real crewmates and projects), but an older hostd: its `info` says proto 1.
final class _OldDemo implements Transport {
  final _inner = demoTransport();

  @override
  Stream<String> get incomingLines => _inner.incomingLines.map((line) {
    final m = jsonDecode(line);
    if (m is Map &&
        m['ok'] == true &&
        (m['data'] as Map?)?.containsKey('hostname') == true) {
      (m['data'] as Map)['proto'] = 1;
      return jsonEncode(m);
    }
    return line;
  });

  @override
  Future<void> sendLine(String line) => _inner.sendLine(line);

  @override
  Future<void> close() => _inner.close();
}

ProviderContainer _container(_Host host) {
  final c = ProviderContainer(
    overrides: [
      transportFactoryProvider.overrideWithValue(host.dial),
      macStorageProvider.overrideWithValue(_FakeMacStorage()),
    ],
  );
  addTearDown(c.dispose);
  return c;
}

void main() {
  final device = MacDevice(nodeId: 'test-node', name: 'Studio Mac');

  group('checkHostProto', () {
    test('a host with no proto is protocol 1 and too old', () {
      final m = checkHostProto(<String, dynamic>{'version': '0.9.0'}, device);
      expect(m, isA<ProtocolMismatch>());
      expect(m!.hostProto, 1);
      expect(m.appProto, kAppProto);
      expect(m.hostIsOlder, isTrue);
      expect(m.deviceName, 'Studio Mac');
    });

    test('a host below the minimum is too old; at the minimum it is fine', () {
      expect(checkHostProto({'proto': kMinHostProto - 1}, device), isNotNull);
      expect(checkHostProto({'proto': kMinHostProto}, device), isNull);
      expect(checkHostProto({'proto': kAppProto + 5}, device), isNull);
    });

    test('a host that needs a newer app than this one says so', () {
      final m = checkHostProto({
        'proto': kAppProto + 1,
        'min_client_proto': kAppProto + 1,
      }, device);
      expect(m, isNotNull);
      expect(m!.hostIsOlder, isFalse);
      expect(m.title, 'Update Atlas Mobile');
    });

    test('a host that still accepts this app is fine', () {
      expect(
        checkHostProto({
          'proto': kAppProto + 1,
          'min_client_proto': kAppProto,
        }, device),
        isNull,
      );
    });

    test('a host that is too old says what to run on it', () {
      final m = checkHostProto({'proto': 1}, device)!;
      expect(m.title, 'Update Atlas on Studio Mac');
      expect(m.message, contains('atlas update'));
    });

    test('a non-integer proto is not trusted as current', () {
      expect(checkHostProto({'proto': 'two'}, device), isNotNull);
    });
  });

  group('ConnectionNotifier protocol check', () {
    test(
      'proto 1: no mux, a mismatch is recorded, nothing is retried',
      () async {
        final host = _Host({'proto': 1, 'version': '0.9.0'});
        final c = _container(host);
        final p = connectionNotifierProvider(device);
        final sub = c.listen(p, (_, _) {});
        addTearDown(sub.close);

        final mux = await c.read(p.future);

        expect(mux, isNull);
        final m = c.read(protocolMismatchProvider(device));
        expect(m, isNotNull);
        expect(m!.hostProto, 1);
        expect(
          host.transports.single.sentLines.single,
          contains('"cmd":"info"'),
        );
        // The backoff would redial after 1 s; a mismatch is not an outage.
        await Future<void>.delayed(const Duration(milliseconds: 1300));
        expect(host.dials, 1, reason: 'no reconnect loop for a version skew');
      },
    );

    test(
      'no proto in info is a mismatch too (a hostd older than the field)',
      () async {
        final host = _Host({'version': '0.9.0'});
        final c = _container(host);
        final p = connectionNotifierProvider(device);
        final sub = c.listen(p, (_, _) {});
        addTearDown(sub.close);

        expect(await c.read(p.future), isNull);
        expect(c.read(protocolMismatchProvider(device))!.hostProto, 1);
      },
    );

    test('proto 2 connects as before and clears nothing', () async {
      final host = _Host({'proto': 2, 'version': '1.0.0'});
      final c = _container(host);
      final p = connectionNotifierProvider(device);
      final sub = c.listen(p, (_, _) {});
      addTearDown(sub.close);

      expect(await c.read(p.future), isNotNull);
      expect(c.read(protocolMismatchProvider(device)), isNull);
      expect(host.dials, 1);
    });

    test(
      'the mismatch is checked again on a manual retry and clears when fixed',
      () async {
        var proto = 1;
        var dials = 0;
        final c = ProviderContainer(
          overrides: [
            transportFactoryProvider.overrideWithValue((_, _) async {
              dials++;
              return _ReplyingTransport({'proto': proto});
            }),
            macStorageProvider.overrideWithValue(_FakeMacStorage()),
          ],
        );
        addTearDown(c.dispose);
        final p = connectionNotifierProvider(device);
        final sub = c.listen(p, (_, _) {});
        addTearDown(sub.close);

        expect(await c.read(p.future), isNull);
        expect(c.read(protocolMismatchProvider(device)), isNotNull);

        proto = 2; // the user ran `atlas update` on the computer
        c.invalidate(p);
        expect(await c.read(p.future), isNotNull);
        expect(c.read(protocolMismatchProvider(device)), isNull);
        expect(dials, 2);
      },
    );

    test(
      'a crew server behind a too-old Mac shows the Mac\'s mismatch',
      () async {
        final host = _Host({'proto': 1});
        final c = _container(host);
        final crew = MacDevice.crew(device, 'build-box', 'Build box');
        final p = connectionNotifierProvider(crew);
        final sub = c.listen(p, (_, _) {});
        addTearDown(sub.close);

        expect(await c.read(p.future), isNull);
        final m = c.read(protocolMismatchProvider(crew));
        expect(m, isNotNull);
        expect(m!.hostIsOlder, isTrue);
        expect(
          host.dials,
          1,
          reason: 'the crew server dials nothing of its own',
        );
      },
    );

    test('foregrounding a mismatched device checks it again', () async {
      var proto = 1;
      var dials = 0;
      final c = ProviderContainer(
        overrides: [
          transportFactoryProvider.overrideWithValue((_, _) async {
            dials++;
            return _ReplyingTransport({'proto': proto});
          }),
          macStorageProvider.overrideWithValue(_FakeMacStorage()),
        ],
      );
      addTearDown(c.dispose);
      final p = connectionNotifierProvider(device);
      final sub = c.listen(p, (_, _) {});
      addTearDown(sub.close);
      expect(await c.read(p.future), isNull);

      proto = 2;
      await c.read(p.notifier).onForegrounded();
      expect(await c.read(p.future), isNotNull);
      expect(dials, 2);
    });
  });

  group('home screen', () {
    Future<void> open(WidgetTester t, _Host host) async {
      await t.binding.setSurfaceSize(const Size(393, 852));
      await t.pumpWidget(
        ProviderScope(
          overrides: [
            macStorageProvider.overrideWithValue(_Storage([device])),
            transportFactoryProvider.overrideWithValue(host.dial),
          ],
          child: MaterialApp(theme: atlasTheme(), home: const HomeScreen()),
        ),
      );
      await _settle(t);
    }

    testWidgets('an old hostd shows one update line, not "Can\'t reach"', (
      t,
    ) async {
      await open(t, _Host({'proto': 1}));

      expect(find.text('Update Atlas on Studio Mac'), findsOneWidget);
      expect(find.textContaining('atlas update'), findsOneWidget);
      expect(find.text("Can't reach Studio Mac"), findsNothing);
      await t.pumpWidget(const SizedBox());
      await _settle(t);
    });

    testWidgets('a hostd that needs a newer app says to update the app', (
      t,
    ) async {
      await open(
        t,
        _Host({'proto': kAppProto + 1, 'min_client_proto': kAppProto + 1}),
      );

      expect(find.text('Update Atlas Mobile'), findsOneWidget);
      expect(find.text("Can't reach Studio Mac"), findsNothing);
      await t.pumpWidget(const SizedBox());
      await _settle(t);
    });
  });
  group('crew tab', () {
    testWidgets(
      'a computer too old to list its crew says to update it, not "No crew yet"',
      (t) async {
        await t.binding.setSurfaceSize(const Size(393, 852));
        const mac = MacDevice(nodeId: 'demo', name: 'Demo MacBook');
        await t.pumpWidget(
          demoWrap(const CrewScreen(), [mac], transport: _OldDemo.new),
        );
        await settle(t);

        expect(find.text('Update Atlas on Demo MacBook'), findsOneWidget);
        expect(find.text('No crew yet'), findsNothing);
        await t.pumpWidget(const SizedBox());
        await settle(t);
      },
    );

    testWidgets('no banner when every computer agrees', (t) async {
      await t.binding.setSurfaceSize(const Size(393, 852));
      await t.pumpWidget(
        demoWrap(const CrewScreen(), [
          const MacDevice(nodeId: 'demo', name: 'Demo MacBook'),
        ]),
      );
      await settle(t);

      expect(find.textContaining('Update Atlas'), findsNothing);
      expect(find.text('Reviewer'), findsWidgets);
      await t.pumpWidget(const SizedBox());
      await settle(t);
    });
  });
}
