import 'dart:async';
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';
import 'package:mobile_scanner/mobile_scanner.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/permissions/permissions.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/storage/mac_storage.dart';
import 'package:atlas_mobile/core/transport/transport.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/pair/pair_screen.dart';

import 'support/fake_permissions.dart';

// The pairing screen is where the camera, the local network and (after it worked) notifications are asked
// for. Each one is explained before the system prompt, and a refusal always leaves a way to pair.

const _link = 'atlas-mobile://pair?node=n1&code=123456&name=Mac';

class _Storage extends MacStorage {
  _Storage() : super(storage: null);
  final saved = <MacDevice>[];
  @override
  Future<String?> loadSecretKey() async => 'ab' * 32;
  @override
  Future<void> upsertDevice(MacDevice d) async => saved.add(d);
}

/// A computer that accepts the pairing code (or refuses it).
class _Computer implements Transport {
  _Computer({this.accepts = true});
  final bool accepts;
  final _lines = StreamController<String>.broadcast();
  @override
  Stream<String> get incomingLines => _lines.stream;
  @override
  Future<void> close() async {}
  @override
  Future<void> sendLine(String line) async {
    final id = (jsonDecode(line) as Map)['id'];
    scheduleMicrotask(
      () => _lines.add(
        jsonEncode({
          'id': id,
          'ok': accepts,
          if (!accepts) 'error': 'no such code',
          'data': {},
        }),
      ),
    );
  }
}

/// The camera view, replaced: a button that "scans" [_link]; [error] is what the real one reports.
ScannerBuilder _scanner({MobileScannerErrorCode? error}) =>
    (context, onDetect, onError) => error != null
    ? onError(context, MobileScannerException(errorCode: error))
    : TextButton(
        key: const ValueKey('scanner'),
        onPressed: () => onDetect(
          const BarcodeCapture(barcodes: [Barcode(rawValue: _link)]),
        ),
        child: const Text('scanner'),
      );

class _Rig {
  _Rig(this.perms, {this.computer});
  final FakePermissions perms;
  final Transport Function()? computer;
  final storage = _Storage();
  var dials = 0;
  String? pasted;
}

Future<_Rig> _pump(
  WidgetTester t,
  FakePermissions perms, {
  ScannerBuilder? scanner,
  Transport Function()? computer,
  String? clipboard,
}) async {
  final rig = _Rig(perms, computer: computer)..pasted = clipboard;
  t.binding.defaultBinaryMessenger.setMockMethodCallHandler(
    SystemChannels.platform,
    (call) async => call.method == 'Clipboard.getData' && clipboard != null
        ? <String, dynamic>{'text': clipboard}
        : null,
  );
  addTearDown(
    () => t.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      SystemChannels.platform,
      null,
    ),
  );
  final router = GoRouter(
    routes: [
      GoRoute(
        path: '/',
        builder: (context, _) => Scaffold(
          body: TextButton(
            onPressed: () => context.push('/pair'),
            child: const Text('open pair'),
          ),
        ),
      ),
      GoRoute(path: '/pair', builder: (_, _) => const PairScreen()),
    ],
  );
  await t.pumpWidget(
    ProviderScope(
      overrides: [
        permissionsProvider.overrideWithValue(perms),
        scannerBuilderProvider.overrideWithValue(scanner ?? _scanner()),
        macStorageProvider.overrideWithValue(rig.storage),
        transportFactoryProvider.overrideWithValue((_, _) async {
          rig.dials++;
          return (computer ?? _Computer.new)();
        }),
      ],
      child: MaterialApp.router(theme: atlasTheme(), routerConfig: router),
    ),
  );
  await t.tap(find.text('open pair'));
  await _settle(t);
  return rig;
}

/// `deviceLabel` (what pairing sends as the phone's name) asks a plugin that never answers under the test
/// clock; on a platform with no plugin it answers at once.
void _test(String name, Future<void> Function(WidgetTester t) body) =>
    testWidgets(name, (t) async {
      debugDefaultTargetPlatformOverride = TargetPlatform.fuchsia;
      try {
        await body(t);
      } finally {
        debugDefaultTargetPlatformOverride = null;
      }
    });

/// Pairing closes its multiplexer (`await sub.cancel()`), which only finishes in real time: let a little pass,
/// then pump the screen.
Future<void> _settle(WidgetTester t) async {
  await t.runAsync(
    () => Future<void>.delayed(const Duration(milliseconds: 20)),
  );
  for (var i = 0; i < 10; i++) {
    await t.pump(const Duration(milliseconds: 100));
  }
}

final _scannerView = find.byKey(const ValueKey('scanner'));

void main() {
  group('camera', () {
    _test('granted: the scanner is the screen, nothing is asked', (t) async {
      final rig = await _pump(t, FakePermissions());
      expect(_scannerView, findsOneWidget);
      expect(rig.perms.requested, isEmpty);
    });

    _test(
      'not asked: the screen explains first, the camera is not opened or requested',
      (t) async {
        final rig = await _pump(
          t,
          FakePermissions(
            states: {AtlasPermission.camera: PermissionState.notAsked},
          ),
        );
        expect(find.text('Allow the camera'), findsOneWidget);
        expect(find.textContaining('QR code on your computer'), findsOneWidget);
        expect(_scannerView, findsNothing);
        expect(rig.perms.requested, isEmpty);
        expect(
          find.text('Paste link'),
          findsOneWidget,
          reason: 'a way to pair without the camera',
        );

        await t.tap(find.text('Continue'));
        await _settle(t);

        expect(rig.perms.requested, [AtlasPermission.camera]);
        expect(_scannerView, findsOneWidget);
      },
    );

    // App Review 5.1.1(iv): on iOS the screen before the camera prompt offers no "Not now". Paste link stays:
    // it is another way to pair, on the feature's own screen, not a way out of the explanation.
    testWidgets(
      'iOS: the camera explanation has only Continue, and Paste link',
      (t) async {
        debugDefaultTargetPlatformOverride = TargetPlatform.iOS;
        try {
          final rig = await _pump(
            t,
            FakePermissions(
              states: {AtlasPermission.camera: PermissionState.notAsked},
            ),
          );
          expect(find.text('Allow the camera'), findsOneWidget);
          expect(find.text('Continue'), findsOneWidget);
          expect(find.text('Not now'), findsNothing);
          expect(find.text('Paste link'), findsOneWidget);
          expect(rig.perms.requested, isEmpty);
        } finally {
          debugDefaultTargetPlatformOverride = null;
        }
      },
    );

    _test(
      'Not now: says what does not work, offers Allow camera and Paste link',
      (t) async {
        final rig = await _pump(
          t,
          FakePermissions(
            states: {AtlasPermission.camera: PermissionState.notAsked},
          ),
        );
        await t.tap(find.text('Not now'));
        await _settle(t);

        expect(rig.perms.requested, isEmpty);
        expect(_scannerView, findsNothing);
        expect(find.text('Allow camera'), findsOneWidget);
        expect(find.text('Paste link'), findsWidgets);

        await t.tap(find.text('Allow camera'));
        await _settle(t);
        expect(
          find.text('Allow the camera'),
          findsOneWidget,
          reason: 'explained again',
        );
        expect(rig.perms.requested, isEmpty);
      },
    );

    _test(
      'blocked: what stops working, Open Settings, Paste link (and it pairs)',
      (t) async {
        final rig = await _pump(
          t,
          FakePermissions(
            states: {AtlasPermission.camera: PermissionState.blocked},
          ),
          clipboard: _link,
        );
        expect(find.text('Camera access is off'), findsOneWidget);
        expect(
          find.textContaining('cannot scan the pairing code'),
          findsOneWidget,
        );
        expect(_scannerView, findsNothing);
        expect(rig.perms.requested, isEmpty);

        await t.tap(find.text('Open Settings'));
        expect(rig.perms.settingsOpened, 1);

        await t.tap(find.widgetWithText(OutlinedButton, 'Paste link'));
        await _settle(t);
        expect(rig.storage.saved.map((d) => d.nodeId), ['n1']);
      },
    );

    _test('restricted by a device policy: no Open Settings, still Paste link', (
      t,
    ) async {
      await _pump(
        t,
        FakePermissions(
          states: {AtlasPermission.camera: PermissionState.restricted},
        ),
      );
      expect(find.text('Restricted on this phone'), findsOneWidget);
      expect(find.text('Open Settings'), findsNothing);
      expect(find.text('Paste link'), findsWidgets);
    });

    _test(
      'the camera reports it was denied (revoked while open): the same off state',
      (t) async {
        await _pump(
          t,
          FakePermissions(),
          scanner: _scanner(error: MobileScannerErrorCode.permissionDenied),
        );
        expect(find.text('Camera access is off'), findsOneWidget);
        expect(find.text('Open Settings'), findsOneWidget);
      },
    );

    _test(
      'no camera (a simulator, a broken one): says so and offers Paste link',
      (t) async {
        await _pump(
          t,
          FakePermissions(),
          scanner: _scanner(error: MobileScannerErrorCode.unsupported),
        );
        expect(find.text('No camera available'), findsOneWidget);
        expect(find.text('Open Settings'), findsNothing);
        expect(find.text('Paste link'), findsWidgets);
      },
    );

    _test(
      'back from Settings with the camera on: the scanner opens by itself',
      (t) async {
        final rig = await _pump(
          t,
          FakePermissions(
            states: {AtlasPermission.camera: PermissionState.blocked},
          ),
        );
        expect(_scannerView, findsNothing);

        rig.perms.states[AtlasPermission.camera] = PermissionState.granted;
        t.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
        t.binding.handleAppLifecycleStateChanged(AppLifecycleState.resumed);
        await _settle(t);

        expect(_scannerView, findsOneWidget);
      },
    );
  });

  group('local network (iOS)', () {
    _test('explained before the first dial; Continue then pairs', (t) async {
      final rig = await _pump(
        t,
        FakePermissions(
          states: {AtlasPermission.localNetwork: PermissionState.notAsked},
        ),
      );
      t.widget<TextButton>(_scannerView).onPressed!();
      await _settle(t);

      expect(find.text('Find your computer on Wi-Fi'), findsOneWidget);
      expect(rig.dials, 0, reason: 'nothing dialled behind the explanation');
      expect(rig.perms.requested, isEmpty);

      await t.tap(find.text('Continue'));
      await _settle(t);

      expect(rig.perms.requested, [AtlasPermission.localNetwork]);
      expect(rig.dials, 1);
      expect(find.text('Paired'), findsOneWidget);
    });

    _test(
      'Not now: pairing still goes on (through the relay), nothing is requested',
      (t) async {
        final rig = await _pump(
          t,
          FakePermissions(
            states: {AtlasPermission.localNetwork: PermissionState.notAsked},
          ),
        );
        t.widget<TextButton>(_scannerView).onPressed!();
        await _settle(t);
        await t.tap(find.text('Not now'));
        await _settle(t);

        expect(rig.perms.requested, isEmpty);
        expect(rig.dials, 1);
        expect(find.text('Paired'), findsOneWidget);
      },
    );

    _test(
      'a pairing that fails says the relay still works and how to allow Local Network',
      (t) async {
        final rig = await _pump(
          t,
          FakePermissions(
            states: {AtlasPermission.localNetwork: PermissionState.granted},
          ),
          computer: () => _Computer(accepts: false),
        );
        t.widget<TextButton>(_scannerView).onPressed!();
        await _settle(t);

        expect(find.text('Could not pair'), findsOneWidget);
        expect(find.textContaining('through the relay'), findsOneWidget);
        await t.tap(find.text('Open Settings'));
        expect(rig.perms.settingsOpened, 1);
      },
    );

    _test('not iOS: no local network explanation, no hint', (t) async {
      final rig = await _pump(
        t,
        FakePermissions(
          states: {AtlasPermission.localNetwork: PermissionState.unavailable},
        ),
        computer: () => _Computer(accepts: false),
      );
      t.widget<TextButton>(_scannerView).onPressed!();
      await _settle(t);

      expect(find.text('Find your computer on Wi-Fi'), findsNothing);
      expect(rig.perms.requested, isEmpty);
      expect(find.textContaining('through the relay'), findsNothing);
      expect(find.text('Open Settings'), findsNothing);
    });
  });

  group('notifications after the first pairing', () {
    Future<_Rig> paired(WidgetTester t, FakePermissions perms) async {
      final rig = await _pump(t, perms);
      t.widget<TextButton>(_scannerView).onPressed!();
      await _settle(t);
      expect(find.text('Paired'), findsOneWidget);
      return rig;
    }

    final off = FakePermissions.new;
    FakePermissions notAsked() =>
        off(states: {AtlasPermission.notifications: PermissionState.notAsked});

    _test('Open explains why before the system prompt; Continue asks', (
      t,
    ) async {
      final rig = await paired(t, notAsked());
      expect(rig.perms.requested, isEmpty, reason: 'not while pairing');

      await t.tap(find.text('Open'));
      await _settle(t);
      expect(find.text('Get notified when you are away'), findsOneWidget);
      expect(find.textContaining('even when Atlas is closed'), findsOneWidget);
      expect(rig.perms.requested, isEmpty);

      await t.tap(find.text('Continue'));
      await _settle(t);
      expect(rig.perms.requested, [AtlasPermission.notifications]);
      expect(find.text('Paired'), findsNothing, reason: 'the screen closed');
    });

    _test(
      'Not now closes the screen with nothing requested, and is not asked again',
      (t) async {
        final perms = notAsked();
        final rig = await paired(t, perms);
        await t.tap(find.text('Open'));
        await _settle(t);
        await t.tap(find.text('Not now'));
        await _settle(t);
        expect(rig.perms.requested, isEmpty);
        expect(find.text('Paired'), findsNothing);

        // Pairing a second computer: the person already said not now.
        await t.tap(find.text('open pair'));
        await _settle(t);
        t.widget<TextButton>(_scannerView).onPressed!();
        await _settle(t);
        await t.tap(find.text('Open'));
        await _settle(t);
        expect(find.text('Get notified when you are away'), findsNothing);
      },
    );

    _test(
      'already on, off in Settings, or not in this build: Open just closes',
      (t) async {
        for (final state in [
          PermissionState.granted,
          PermissionState.blocked,
          PermissionState.unavailable,
        ]) {
          final rig = await paired(
            t,
            FakePermissions(states: {AtlasPermission.notifications: state}),
          );
          await t.tap(find.text('Open'));
          await _settle(t);
          expect(
            find.text('Get notified when you are away'),
            findsNothing,
            reason: '$state',
          );
          expect(rig.perms.requested, isEmpty, reason: '$state');
          expect(find.text('Paired'), findsNothing, reason: '$state');
        }
      },
    );
  });
}
