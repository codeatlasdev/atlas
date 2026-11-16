import 'dart:math';

import 'package:material_ui/material_ui.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:mobile_scanner/mobile_scanner.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/permissions/permission_ui.dart';
import 'package:atlas_mobile/core/permissions/permissions.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/protocol/multiplexer.dart';
import 'package:atlas_mobile/core/device_label.dart';
import 'package:atlas_mobile/core/protocol/pair_info.dart';
import 'package:atlas_mobile/core/storage/mac_storage.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/core/ui/computer_state.dart';

/// State for the pairing flow.
enum PairStatus { idle, connecting, success, error }

// Not using freezed; simple class is sufficient.
class PairState {
  const PairState({this.status = PairStatus.idle, this.error});
  final PairStatus status;
  final String? error;
}

final pairNotifierProvider =
    NotifierProvider.autoDispose<PairNotifier, PairState>(PairNotifier.new);

class PairNotifier extends Notifier<PairState> {
  @override
  PairState build() => const PairState();

  /// Start pairing from a QR-scanned URL.
  Future<void> pair(String qrUrl) async {
    state = const PairState(status: PairStatus.connecting);
    // Read before the first await: leaving the screen disposes this notifier, but a
    // pairing the computer accepted must still be saved.
    final storage = ref.read(macStorageProvider);
    final connect = ref.read(transportFactoryProvider);
    void report(PairState s) {
      if (ref.mounted) state = s;
    }

    try {
      final info = PairInfo.parse(qrUrl);
      final secretKey = await _ensureSecretKey(storage);
      final device = MacDevice(
        nodeId: info.nodeId,
        name: info.name,
        addrs: info.addrs,
        relay: info.relay,
      );

      // One-shot: dial, send `pair`, close. The long-lived connection is opened by
      // ConnectionNotifier once the Mac is in the paired list.
      final transport = await connect(device, secretKey);
      final mux = Multiplexer(transport);
      try {
        final resp = await mux.call('pair', {
          'code': info.code,
          // How the computer lists this phone (its model and system), so two phones never look alike there.
          'name': await deviceLabel(),
        });
        if (resp['ok'] != true) {
          throw Exception(resp['error'] ?? 'Pairing failed');
        }
      } finally {
        await mux.close();
      }
      await storage.upsertDevice(device);
      report(const PairState(status: PairStatus.success));
    } catch (e) {
      report(PairState(status: PairStatus.error, error: e.toString()));
    }
  }

  Future<String> _ensureSecretKey(MacStorage storage) async {
    var key = await storage.loadSecretKey();
    if (key == null) {
      // Generate 32 random bytes and hex-encode.
      final rng = Random.secure();
      final bytes = List<int>.generate(32, (_) => rng.nextInt(256));
      key = bytes.map((b) => b.toRadixString(16).padLeft(2, '0')).join();
      await storage.saveSecretKey(key);
    }
    return key;
  }
}

/// The camera view: `MobileScanner`, which tests replace (it needs a camera and its plugin). [onError] draws
/// what the camera reports when it cannot open (denied, no camera).
typedef ScannerBuilder = Widget Function(
  BuildContext context,
  void Function(BarcodeCapture) onDetect,
  Widget Function(BuildContext, MobileScannerException) onError,
);

final scannerBuilderProvider = Provider<ScannerBuilder>(
  (ref) =>
      (context, onDetect, onError) =>
          MobileScanner(onDetect: onDetect, errorBuilder: onError),
);

/// QR scanning + pairing screen.
class PairScreen extends ConsumerStatefulWidget {
  const PairScreen({super.key});

  @override
  ConsumerState<PairScreen> createState() => _PairScreenState();
}

class _PairScreenState extends ConsumerState<PairScreen> {
  bool _scanned = false;

  /// "Not now" on the camera's explanation: the screen then says what it needs and offers the way around.
  bool _declined = false;

  void _onDetect(BarcodeCapture capture) {
    final raw = capture.barcodes.firstOrNull?.rawValue;
    if (raw == null) return;
    _begin(raw);
  }

  /// Pairs with [url], after telling the person (iOS, the first time) why the phone is about to look for the
  /// computer on the Wi-Fi. "Not now" still pairs: through the relay.
  Future<void> _begin(String url) async {
    if (_scanned) return;
    setState(() => _scanned = true);
    await askPermission(context, ref, AtlasPermission.localNetwork);
    if (!mounted) return;
    ref.read(pairNotifierProvider.notifier).pair(url);
  }

  Future<void> _done() async {
    // Home lists paired Macs from storage: reload it. A real computer replaces the demo.
    ref.invalidate(pairedDevicesProvider);
    ref.read(demoModeProvider.notifier).state = false;
    // The first pairing is when approvals reaching the phone start to matter: ask, with the why.
    await offerNotifications(context, ref);
    if (mounted) context.pop();
  }

  /// No computer at hand (or App Review): the home shows the in-app demo computer.
  void _demo() {
    ref.read(demoModeProvider.notifier).state = true;
    context.pop();
  }

  void _retry() {
    _scanned = false;
    ref.invalidate(pairNotifierProvider);
  }

  Future<void> _paste() async {
    final text = (await Clipboard.getData(Clipboard.kTextPlain))?.text?.trim();
    if (!mounted || _scanned) return;
    if (text == null || !text.startsWith('atlas-mobile://pair')) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(
          content: Text('Copy the atlas-mobile://pair link first.'),
        ),
      );
      return;
    }
    _begin(text);
  }

  /// The system prompt, after the inline explanation's "Continue".
  Future<void> _allowCamera() async {
    await ref.read(permissionsProvider).request(AtlasPermission.camera);
    ref.invalidate(permissionStateProvider(AtlasPermission.camera));
  }

  @override
  Widget build(BuildContext context) {
    final st = ref.watch(pairNotifierProvider);
    final c = AtlasColors.of(context);
    final camera = ref.watch(permissionStateProvider(AtlasPermission.camera));
    final scanning = !_scanned && st.status == PairStatus.idle;
    final live = scanning && camera.value == PermissionState.granted;
    // The explanation has no Paste link of its own; every state that says "off" does.
    final barPaste =
        live ||
        (scanning && camera.value == PermissionState.notAsked && !_declined);
    return PermissionResumeRefresh(
      onResume: () =>
          ref.invalidate(permissionStateProvider(AtlasPermission.camera)),
      child: Scaffold(
        backgroundColor: live ? Colors.black : null,
        extendBodyBehindAppBar: live,
        appBar: AppBar(
          title: const Text('Pair a computer'),
          backgroundColor: live ? Colors.transparent : null,
          foregroundColor: live ? Colors.white : null,
          actions: [
            // No camera (a simulator) or the code is on screen elsewhere: paste the link
            // printed under the QR.
            if (barPaste)
              TextButton.icon(
                style: live
                    ? TextButton.styleFrom(foregroundColor: Colors.white)
                    : null,
                icon: const Icon(Icons.content_paste_rounded, size: 18),
                label: const Text('Paste link'),
                onPressed: _paste,
              ),
          ],
        ),
        body: AnimatedSwitcher(
          duration: Motion.slow,
          child: switch (st.status) {
            _ when scanning && camera.value == null => const SizedBox.expand(
              key: ValueKey('camera-checking'),
            ),
            _ when live => _Scanner(
              key: const ValueKey('scan'),
              onDetect: _onDetect,
              onPaste: _paste,
              onDemo: _demo,
            ),
            _ when scanning && camera.value == PermissionState.notAsked =>
              _declined
                  ? _Centered(
                      key: const ValueKey('camera-needed'),
                      child: _CameraNeeded(
                        onAllow: () => setState(() => _declined = false),
                        onPaste: _paste,
                        onDemo: _demo,
                      ),
                    )
                  : _Centered(
                      key: const ValueKey('camera-explain'),
                      child: PermissionExplainer(
                        permission: AtlasPermission.camera,
                        why: 'Atlas reads the QR code on your computer to pair with it.',
                        onContinue: _allowCamera,
                        onNotNow: () => setState(() => _declined = true),
                      ),
                    ),
            _ when scanning => _Centered(
              key: const ValueKey('camera-off'),
              child: _CameraOff(
                state: camera.value ?? PermissionState.unavailable,
                onPaste: _paste,
                onDemo: _demo,
              ),
            ),
            PairStatus.success => EmptyState(
              key: const ValueKey('ok'),
              icon: _Burst(color: c.success),
              title: 'Paired',
              message: 'This phone can now reach your computer from anywhere, while it is awake.',
              action: FilledButton(onPressed: _done, child: const Text('Open')),
            ),
            PairStatus.error => EmptyState(
              key: const ValueKey('err'),
              icon: Icon(Icons.link_off_rounded, size: 56, color: c.error),
              title: 'Could not pair',
              message: st.error ?? 'Unknown error',
              action: Column(
                children: [
                  FilledButton(
                    onPressed: _retry,
                    child: const Text('Scan again'),
                  ),
                  TextButton.icon(
                    icon: const Icon(Icons.copy_rounded, size: 16),
                    label: const Text('Copy error'),
                    onPressed: () {
                      Clipboard.setData(ClipboardData(text: st.error ?? ''));
                      ScaffoldMessenger.of(context).showSnackBar(
                        const SnackBar(content: Text('Error copied')),
                      );
                    },
                  ),
                  const _LocalNetworkHint(),
                ],
              ),
            ),
            _ => const ReachingView(key: ValueKey('busy'), connecting: true),
          },
        ),
      ),
    );
  }
}

/// A card centred and scrollable: the explanation and the "off" states are the whole screen.
class _Centered extends StatelessWidget {
  const _Centered({super.key, required this.child});
  final Widget child;

  @override
  Widget build(BuildContext context) => Center(
    child: SingleChildScrollView(
      padding: const EdgeInsets.all(Space.xl),
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 420),
        child: child,
      ),
    ),
  );
}

/// The camera is off for good (refused, policy, none): what that means, Open Settings where it helps, and
/// the ways to pair without it.
class _CameraOff extends StatelessWidget {
  const _CameraOff({
    required this.state,
    required this.onPaste,
    required this.onDemo,
  });
  final PermissionState state;
  final VoidCallback onPaste;
  final VoidCallback onDemo;

  @override
  Widget build(BuildContext context) => ColoredBox(
    color: AtlasColors.of(context).ground,
    child: _Centered(
      child: PermissionOffCard(
        permission: AtlasPermission.camera,
        state: state,
        extra: [
          OutlinedButton.icon(
            icon: const Icon(Icons.content_paste_rounded, size: 18),
            label: const Text('Paste link'),
            onPressed: onPaste,
          ),
          TextButton(
            onPressed: onDemo,
            child: const Text('No computer at hand? Explore a demo'),
          ),
        ],
      ),
    ),
  );
}

/// "Not now" to the camera: nothing is wrong, but the screen has nothing to scan with. Allow it, or paste.
class _CameraNeeded extends StatelessWidget {
  const _CameraNeeded({
    required this.onAllow,
    required this.onPaste,
    required this.onDemo,
  });
  final VoidCallback onAllow;
  final VoidCallback onPaste;
  final VoidCallback onDemo;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final t = Theme.of(context).textTheme;
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Icon(Icons.qr_code_scanner_rounded, size: 48, color: c.muted),
        const SizedBox(height: Space.lg),
        Text(
          'The camera scans the pairing code',
          style: t.titleMedium,
          textAlign: TextAlign.center,
        ),
        const SizedBox(height: Space.sm),
        Text(
          'Allow it, or paste the link printed under the QR code on your computer.',
          style: t.bodyLarge!.copyWith(color: c.muted, height: 1.4),
          textAlign: TextAlign.center,
        ),
        const SizedBox(height: Space.xl),
        FilledButton(onPressed: onAllow, child: const Text('Allow camera')),
        const SizedBox(height: Space.xs),
        OutlinedButton.icon(
          icon: const Icon(Icons.content_paste_rounded, size: 18),
          label: const Text('Paste link'),
          onPressed: onPaste,
        ),
        TextButton(
          onPressed: onDemo,
          child: const Text('No computer at hand? Explore a demo'),
        ),
      ],
    );
  }
}

/// Under a failed pairing, on iOS: the one thing the system hides from the app is that Local Network was
/// refused, so say what it means (the relay still pairs) and where to change it.
class _LocalNetworkHint extends ConsumerWidget {
  const _LocalNetworkHint();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final state = ref
        .watch(permissionStateProvider(AtlasPermission.localNetwork))
        .value;
    if (state == null || state == PermissionState.unavailable) {
      return const SizedBox.shrink();
    }
    final c = AtlasColors.of(context);
    return Padding(
      padding: const EdgeInsets.only(top: Space.lg),
      child: Column(
        children: [
          Text(
            'If you chose Don\'t Allow for Local Network, pairing still works through the relay. '
            'To connect straight over Wi-Fi, allow it in Settings.',
            textAlign: TextAlign.center,
            style: Theme.of(context).textTheme.bodyMedium!
                .copyWith(color: c.muted, height: 1.4),
          ),
          TextButton(
            onPressed: ref.read(permissionsProvider).openSettings,
            child: const Text('Open Settings'),
          ),
        ],
      ),
    );
  }
}

/// Camera with a framed target, the hint where the code is and, under it, the demo. When the camera reports
/// it cannot open (switched off while the screen was open, no camera at all) the screen becomes
/// [_CameraOff]: what that means and the ways to pair without it.
class _Scanner extends ConsumerStatefulWidget {
  const _Scanner({
    super.key,
    required this.onDetect,
    required this.onPaste,
    required this.onDemo,
  });
  final void Function(BarcodeCapture) onDetect;
  final VoidCallback onPaste;
  final VoidCallback onDemo;

  @override
  ConsumerState<_Scanner> createState() => _ScannerState();
}

class _ScannerState extends ConsumerState<_Scanner> {
  MobileScannerException? _error;

  /// Called while the scanner builds: the screen changes after this frame, not inside it.
  Widget _failed(BuildContext context, MobileScannerException e) {
    if (_error == null) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) setState(() => _error = e);
      });
    }
    return const SizedBox.shrink();
  }

  @override
  Widget build(BuildContext context) {
    final error = _error;
    if (error != null) {
      return _CameraOff(
        state: error.errorCode == MobileScannerErrorCode.permissionDenied
            ? PermissionState.blocked
            : PermissionState.unavailable,
        onPaste: widget.onPaste,
        onDemo: widget.onDemo,
      );
    }
    final c = AtlasColors.of(context);
    return Stack(
      fit: StackFit.expand,
      children: [
        ref.watch(scannerBuilderProvider)(context, widget.onDetect, _failed),
        CustomPaint(painter: _FramePainter(c.accent)),
        Positioned(
          left: Space.xl,
          right: Space.xl,
          bottom: Space.xxl + MediaQuery.paddingOf(context).bottom,
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Container(
                padding: const EdgeInsets.all(Space.lg),
                decoration: BoxDecoration(
                  color: Colors.black.withValues(alpha: .6),
                  borderRadius: BorderRadius.circular(Space.radius),
                ),
                child: Text(
                  'On your computer: Atlas Desktop → Settings → Mobile → Pair a phone',
                  textAlign: TextAlign.center,
                  style: Theme.of(context).textTheme.bodyLarge!
                      .copyWith(color: Colors.white, height: 1.4),
                ),
              ),
              const SizedBox(height: Space.sm),
              // Sample data that connects to nothing: for App Review and anyone without
              // Atlas on a computer yet.
              TextButton(
                style: TextButton.styleFrom(foregroundColor: Colors.white70),
                onPressed: widget.onDemo,
                child: const Text('No computer at hand? Explore a demo'),
              ),
            ],
          ),
        ),
      ],
    );
  }
}

/// Darkens around a centered square and draws accent corners on it.
class _FramePainter extends CustomPainter {
  _FramePainter(this.color);
  final Color color;

  @override
  void paint(Canvas canvas, Size size) {
    final side = size.width * .68;
    final r = Rect.fromCenter(
      center: size.center(Offset.zero) - const Offset(0, 40),
      width: side,
      height: side,
    );
    final hole = RRect.fromRectAndRadius(r, const Radius.circular(24));
    canvas.drawPath(
      Path.combine(
        PathOperation.difference,
        Path()..addRect(Offset.zero & size),
        Path()..addRRect(hole),
      ),
      Paint()..color = Colors.black.withValues(alpha: .55),
    );
    final p = Paint()
      ..color = color
      ..style = PaintingStyle.stroke
      ..strokeWidth = 4
      ..strokeCap = StrokeCap.round;
    const l = 34.0;
    for (final (dx, dy) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)]) {
      final x = r.left + r.width * dx, y = r.top + r.height * dy;
      final sx = dx == 0 ? 1 : -1, sy = dy == 0 ? 1 : -1;
      canvas.drawLine(
        Offset(x, y + sy * 24),
        Offset(x, y + sy * l + sy * 24),
        p,
      );
      canvas.drawLine(
        Offset(x + sx * 24, y),
        Offset(x + sx * l + sx * 24, y),
        p,
      );
      canvas.drawArc(
        Rect.fromCircle(center: Offset(x + sx * 24, y + sy * 24), radius: 24),
        _startAngle(dx, dy),
        1.5708,
        false,
        p,
      );
    }
  }

  double _startAngle(double dx, double dy) => switch ((dx, dy)) {
    (0, 0) => 3.1416,
    (1, 0) => -1.5708,
    (0, 1) => 1.5708,
    _ => 0,
  };

  @override
  bool shouldRepaint(_FramePainter old) => old.color != color;
}

/// Success mark: a check that scales in over a soft glow.
class _Burst extends StatelessWidget {
  const _Burst({required this.color});
  final Color color;

  @override
  Widget build(BuildContext context) => TweenAnimationBuilder<double>(
    tween: Tween(begin: 0, end: 1),
    duration: const Duration(milliseconds: 600),
    curve: const Cubic(.2, .9, .3, 1.25),
    builder: (_, t, _) => Container(
      width: 112,
      height: 112,
      decoration: BoxDecoration(
        shape: BoxShape.circle,
        gradient: RadialGradient(
          colors: [
            color.withValues(alpha: .3 * t),
            color.withValues(alpha: 0),
          ],
        ),
      ),
      child: Transform.scale(
        scale: .5 + .5 * t,
        child: Icon(Icons.check_circle_rounded, size: 64, color: color),
      ),
    ),
  );
}
