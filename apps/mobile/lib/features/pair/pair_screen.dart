import 'dart:math';

import 'package:go_router/go_router.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:mobile_scanner/mobile_scanner.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/atlas_logo.dart';
import 'package:atlas_mobile/core/storage/mac_storage.dart';
import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/features/home/home_screen.dart';
import 'package:atlas_mobile/core/protocol/multiplexer.dart';

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
  Future<void> pair(String qrUrl, WidgetRef widgetRef) async {
    state = const PairState(status: PairStatus.connecting);
    try {
      final info = PairInfo.parse(qrUrl);
      final storage = ref.read(macStorageProvider);
      final secretKey = await _ensureSecretKey(storage);
      final device = MacDevice(
        nodeId: info.nodeId,
        name: info.name,
        addrs: info.addrs,
        relay: info.relay,
      );

      // One-shot: dial, send `pair`, close. The long-lived connection is opened by
      // ConnectionNotifier once the Mac is in the paired list.
      final transport = await ref.read(transportFactoryProvider)(
        device,
        secretKey,
      );
      final mux = Multiplexer(transport);
      try {
        final resp = await mux.call('pair', {
          'code': info.code,
          'name': 'Atlas Mobile',
        });
        if (resp['ok'] != true) {
          throw Exception(resp['error'] ?? 'Pairing failed');
        }
      } finally {
        await mux.close();
      }
      await storage.upsertDevice(device);
      state = const PairState(status: PairStatus.success);
    } catch (e) {
      state = PairState(status: PairStatus.error, error: e.toString());
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

/// QR scanning + pairing screen.
class PairScreen extends ConsumerStatefulWidget {
  const PairScreen({super.key});

  @override
  ConsumerState<PairScreen> createState() => _PairScreenState();
}

class _PairScreenState extends ConsumerState<PairScreen> {
  bool _scanned = false;

  void _onDetect(BarcodeCapture capture) {
    final raw = capture.barcodes.firstOrNull?.rawValue;
    if (raw == null || _scanned) return;
    _scanned = true;
    ref.read(pairNotifierProvider.notifier).pair(raw, ref);
  }

  void _done() {
    // Home lists paired Macs from storage: reload it.
    ref.invalidate(pairedDevicesProvider);
    context.pop();
  }

  void _retry() {
    _scanned = false;
    ref.invalidate(pairNotifierProvider);
  }

  @override
  Widget build(BuildContext context) {
    final st = ref.watch(pairNotifierProvider);
    final c = AtlasColors.of(context);
    final scanning = !_scanned && st.status == PairStatus.idle;
    return Scaffold(
      backgroundColor: scanning ? Colors.black : null,
      extendBodyBehindAppBar: scanning,
      appBar: AppBar(
        title: const Text('Pair a Mac'),
        backgroundColor: scanning ? Colors.transparent : null,
        foregroundColor: scanning ? Colors.white : null,
      ),
      body: AnimatedSwitcher(
        duration: Motion.slow,
        child: switch (st.status) {
          _ when scanning => _Scanner(
            key: const ValueKey('scan'),
            onDetect: _onDetect,
          ),
          PairStatus.success => EmptyState(
            key: const ValueKey('ok'),
            icon: _Burst(color: c.success),
            title: 'Paired',
            message: 'This phone can now reach your Mac from anywhere, while it is awake.',
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
              ],
            ),
          ),
          _ => const EmptyState(
            key: ValueKey('busy'),
            icon: AtlasLogo(size: 96, animated: true),
            title: 'Reaching your Mac…',
            message:
                'Directly when possible, through an encrypted relay otherwise.',
          ),
        },
      ),
    );
  }
}

/// Camera with a framed target and the hint where the code is.
class _Scanner extends StatelessWidget {
  const _Scanner({super.key, required this.onDetect});
  final void Function(BarcodeCapture) onDetect;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Stack(
      fit: StackFit.expand,
      children: [
        MobileScanner(onDetect: onDetect),
        CustomPaint(painter: _FramePainter(c.accent)),
        Positioned(
          left: Space.xl,
          right: Space.xl,
          bottom: Space.xxl + MediaQuery.paddingOf(context).bottom,
          child: Container(
            padding: const EdgeInsets.all(Space.lg),
            decoration: BoxDecoration(
              color: Colors.black.withValues(alpha: .6),
              borderRadius: BorderRadius.circular(Space.radius),
            ),
            child: const Text(
              'On your Mac: Atlas Desktop → Settings → Mobile → Pair a phone',
              textAlign: TextAlign.center,
              style: TextStyle(color: Colors.white, fontSize: 15, height: 1.4),
            ),
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
