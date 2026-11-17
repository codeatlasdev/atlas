import 'dart:async';
import 'dart:io';

import 'package:material_ui/material_ui.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/permissions/permissions.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/push/push_provider.dart';
import 'package:atlas_mobile/core/storage/mac_storage.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/motion_size.dart';
import 'package:atlas_mobile/features/settings/settings_screen.dart';

import 'support/frame_probe.dart';
import 'support/fake_permissions.dart';

// Content that grows into place (a status line arriving, a prompt above a composer) must not jump in one frame
// with motion on, and must just be there with Reduce motion: an `AnimatedSize` given `Duration.zero` re-dirties
// itself while it lays out (found by #405 in the agent chat; Settings had the same code), and an `AnimatedSize`
// with a fixed duration ignored Reduce motion (crew thread, onboarding).

class _Storage extends MacStorage {
  _Storage() : super(storage: null);
  @override
  Future<List<MacDevice>> loadDevices() async => const [];
  @override
  Future<String?> loadSecretKey() async => 'ab';
}

Widget _app(Widget home, {required bool reduce}) => MaterialApp(
  theme: atlasTheme(),
  builder: (context, child) => MediaQuery(
    data: MediaQuery.of(context).copyWith(disableAnimations: reduce),
    child: child!,
  ),
  home: home,
);

/// Settings whose notification line arrives after the first frame.
Future<Completer<String?>> _settings(
  WidgetTester t, {
  required bool reduce,
}) async {
  final status = Completer<String?>();
  await t.pumpWidget(
    ProviderScope(
      overrides: [
        macStorageProvider.overrideWithValue(_Storage()),
        pushRegistrationProvider.overrideWith((ref) => status.future),
        // Only notifications exist on this phone, and the row arrives with the answer.
        permissionsProvider.overrideWithValue(
          FakePermissions(
            states: {
              AtlasPermission.camera: PermissionState.unavailable,
              AtlasPermission.localNetwork: PermissionState.unavailable,
            },
          ),
        ),
        permissionStateProvider(AtlasPermission.notifications).overrideWith(
          (ref) => status.future.then((_) => PermissionState.granted),
        ),
      ],
      child: _app(const SettingsScreen(), reduce: reduce),
    ),
  );
  await t.pump();
  return status;
}

void main() {
  final privacy = {'privacy': find.text('Privacy policy')};

  testWidgets(
    'Reduce motion: the notification line is there in the frame it arrives, the rows below move once',
    (t) async {
      final status = await _settings(t, reduce: true);
      final before = t.getTopLeft(find.text('Privacy policy')).dy;
      status.complete('Notifications are on');
      final probe = FrameProbe(t, privacy);
      await probe.record(const Duration(milliseconds: 300));
      expect(t.takeException(), isNull);
      expect(find.text('Notifications are on'), findsOneWidget);
      // One step, on the first frame: never a glide under Reduce motion.
      final tops = [for (final f in probe.frames) f['privacy']!.rect!.top];
      expect(
        tops.first,
        greaterThan(before),
        reason: 'the line was not laid out on its first frame',
      );
      expect(
        tops.toSet(),
        hasLength(1),
        reason: 'Privacy policy kept moving after the first frame: $tops',
      );
    },
  );

  testWidgets(
    'motion on: the notification line opens over frames, never one jump',
    (t) async {
      final status = await _settings(t, reduce: false);
      final before = t.getTopLeft(find.text('Privacy policy')).dy;
      status.complete('Notifications are on');
      final probe = FrameProbe(t, privacy);
      await probe.record(const Duration(milliseconds: 400));
      final tops = [for (final f in probe.frames) f['privacy']!.rect!.top];
      final total = tops.last - before;
      expect(total, greaterThan(20));
      var biggest = 0.0;
      var prev = before;
      for (final y in tops) {
        biggest = (y - prev).abs() > biggest ? (y - prev).abs() : biggest;
        prev = y;
      }
      expect(
        biggest,
        lessThan(total * 0.6),
        reason: 'moved $total px, $biggest of them in one frame: $tops',
      );
    },
  );

  testWidgets('MotionSize with Reduce motion adds no animation of its own', (
    t,
  ) async {
    final show = ValueNotifier(false);
    await t.pumpWidget(
      _app(
        Scaffold(
          body: ValueListenableBuilder<bool>(
            valueListenable: show,
            builder: (_, s, _) => Column(
              children: [
                MotionSize(
                  child: s
                      ? const SizedBox(height: 80)
                      : const SizedBox.shrink(),
                ),
                const SizedBox(key: ValueKey('below'), height: 10, width: 10),
              ],
            ),
          ),
        ),
        reduce: true,
      ),
    );
    show.value = true;
    final probe = FrameProbe(t, {'below': find.byKey(const ValueKey('below'))});
    await probe.record(const Duration(milliseconds: 300));
    probe.expectNoShift('below');
    expect(probe.frames.first['below']!.rect!.top, 80);
    expect(t.hasRunningAnimations, isFalse);
  });

  // One way to grow content: AnimatedSize is only used inside MotionSize.
  test('no AnimatedSize outside MotionSize', () {
    final offenders = <String>[];
    for (final f in Directory(
      'lib',
    ).listSync(recursive: true).whereType<File>()) {
      if (!f.path.endsWith('.dart') || f.path.endsWith('motion_size.dart')) {
        continue;
      }
      final lines = f.readAsLinesSync();
      for (var i = 0; i < lines.length; i++) {
        if (lines[i].trimLeft().startsWith('//')) continue;
        if (lines[i].contains('AnimatedSize(')) {
          offenders.add('${f.path}:${i + 1}');
        }
      }
    }
    expect(
      offenders,
      isEmpty,
      reason: 'use MotionSize (core/ui/motion_size.dart): $offenders',
    );
  });
}
