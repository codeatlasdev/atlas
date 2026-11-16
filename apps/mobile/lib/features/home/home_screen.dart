import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_riverpod/legacy.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/demo/demo_computer.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/atlas_logo.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/features/home/dashboard.dart';
import 'package:atlas_mobile/features/home/splash.dart';
import 'package:atlas_mobile/features/projects/projects_provider.dart';

/// Strips the trailing `.local` mDNS suffix and converts hyphens to spaces
/// for friendlier display: "MacBook-Pro-de-Luan.local" → "MacBook Pro de Luan".
String friendlyDeviceName(String raw) {
  final stripped = raw.endsWith('.local')
      ? raw.substring(0, raw.length - 6)
      : raw;
  return stripped.replaceAll('-', ' ');
}

/// While the home is not ready: (stage caption, or null while reading storage).
typedef _Loading = ({String? caption});

/// Debug builds only: show the onboarding on every launch, even with a paired computer.
/// `--dart-define=ATLAS_ALWAYS_ONBOARD=false` turns it off; release builds never do it.
const _alwaysOnboard =
    kDebugMode &&
    bool.fromEnvironment('ATLAS_ALWAYS_ONBOARD', defaultValue: true);

/// Onboarding was finished in this launch (debug: lets the dashboard show afterwards).
final onboardingSeenProvider = StateProvider<bool>((ref) => false);

/// The app is in its first run: no computer paired (or, in debug, the onboarding not finished this launch),
/// so the shell shows the onboarding full screen instead of the tabs. Null while storage is still read (the
/// home's loading screen covers that); false in demo mode and when storage failed (the home shows the error).
final firstRunProvider = Provider.autoDispose<bool?>((ref) {
  if (ref.watch(demoModeProvider)) return false;
  final devices = ref.watch(pairedDevicesProvider);
  if (devices.hasError) return false;
  if (!devices.hasValue) return null;
  return devices.value!.isEmpty ||
      (!ref.watch(onboardingSeenProvider) && _alwaysOnboard);
});

/// Home: the paired computer's dashboard (the first run's onboarding is the shell's, not this tab's).
class HomeScreen extends ConsumerStatefulWidget {
  const HomeScreen({super.key});

  @override
  ConsumerState<HomeScreen> createState() => _HomeScreenState();
}

class _HomeScreenState extends ConsumerState<HomeScreen> {
  /// The computer shown; the first paired one until the user switches.
  String? _nodeId;

  /// When the current loading screen appeared: it stays until the logo has formed.
  DateTime? _splashSince;
  Timer? _splashTimer;

  /// The last stage shown, kept while the logo finishes forming.
  String? _caption;

  /// Computers whose dashboard has shown: a later reconnect is handled inside it.
  final _ready = <String>{};

  @override
  void dispose() {
    _splashTimer?.cancel();
    super.dispose();
  }

  /// The computer shown (the demo one, or the chosen paired one) and the others to switch to. No device:
  /// nothing is paired, the shell is showing the onboarding over this tab.
  ({MacDevice? device, List<MacDevice> others}) _target(
    List<MacDevice> stored,
  ) {
    if (ref.watch(demoModeProvider)) {
      return (device: demoDevice, others: const []);
    }
    if (stored.isEmpty) return (device: null, others: const []);
    final device = stored.firstWhere(
      (d) => d.nodeId == _nodeId,
      orElse: () => stored.first,
    );
    return (
      device: device,
      others: [
        for (final d in stored)
          if (d.nodeId != device.nodeId) d,
      ],
    );
  }

  /// One loading screen from launch until the dashboard has data: the connection and the
  /// first project list are watched here so the logo is not torn down between stages.
  _Loading? _loading(AsyncValue<List<MacDevice>> devices, MacDevice? device) {
    if (devices.isLoading && !devices.hasValue) return (caption: null);
    if (device == null || _ready.contains(device.nodeId)) return null;
    final conn = ref.watch(connectionNotifierProvider(device));
    if (conn.value == null) {
      return conn.isLoading ? (caption: 'Reaching ${device.name}…') : null;
    }
    final first = ref.watch(projectListProvider(device));
    if (first.isLoading && !first.hasValue) {
      return (caption: 'Loading projects…');
    }
    return null;
  }

  @override
  Widget build(BuildContext context) {
    final devices = ref.watch(pairedDevicesProvider);
    final target = devices.hasValue ? _target(devices.value!) : null;
    final loading = devices.hasError ? null : _loading(devices, target?.device);

    // Hold the loading screen until the globe has formed, so it never flashes halfway.
    if (loading != null) {
      _splashSince ??= DateTime.now();
      _caption = loading.caption ?? _caption;
    } else if (_splashSince != null) {
      final left = logoEntrance - DateTime.now().difference(_splashSince!);
      if (left > Duration.zero) {
        _splashTimer ??= Timer(left, () {
          if (mounted) setState(() => _splashSince = _splashTimer = null);
        });
      } else {
        _splashSince = null;
        _splashTimer = null;
      }
    }
    final splash = loading != null || _splashSince != null;
    if (!splash && target?.device != null) _ready.add(target!.device!.nodeId);

    return Scaffold(
      // One screen at a time: the loading screen (or the other computer's dashboard) is gone on the first
      // frame and the next one fades in over the ground, never both superimposed.
      body: AnimatedSwitcher(
        duration: Motion.reduced(context) ? Duration.zero : Motion.fast,
        switchOutCurve: const Threshold(1),
        child: splash
            ? HomeSplash(key: const ValueKey('splash'), caption: _caption)
            : devices.hasError
            ? EmptyState(
                title: 'Could not read paired computers',
                message: '${devices.error}',
                action: OutlinedButton(
                  onPressed: () => ref.invalidate(pairedDevicesProvider),
                  child: const Text('Try again'),
                ),
              )
            : target!.device == null
            // Nothing paired: the shell shows the onboarding over this tab, which stays bare.
            ? const SizedBox.shrink(key: ValueKey('unpaired'))
            : HomeDashboard(
                key: ValueKey(target.device!.nodeId),
                device: target.device!,
                others: target.others,
                onSwitch: (d) => setState(() => _nodeId = d.nodeId),
              ),
      ),
    );
  }
}
