import 'dart:math';

import 'package:go_router/go_router.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:mobile_scanner/mobile_scanner.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
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

  @override
  Widget build(BuildContext context) {
    final pairState = ref.watch(pairNotifierProvider);

    return Scaffold(
      appBar: AppBar(title: const Text('Pair a Mac')),
      body: Stack(
        children: [
          if (!_scanned && pairState.status == PairStatus.idle) ...[
            MobileScanner(
              onDetect: (capture) {
                final barcode = capture.barcodes.firstOrNull;
                if (barcode?.rawValue != null && !_scanned) {
                  _scanned = true;
                  ref
                      .read(pairNotifierProvider.notifier)
                      .pair(barcode!.rawValue!, ref);
                }
              },
            ),
            const Align(
              alignment: Alignment.bottomCenter,
              child: Padding(
                padding: EdgeInsets.all(24),
                child: Text(
                  'Scan the QR code shown in\nAtlas Desktop → Settings → Mobile Devices',
                  textAlign: TextAlign.center,
                  style: TextStyle(color: Colors.white, fontSize: 16),
                ),
              ),
            ),
          ],
          if (pairState.status == PairStatus.connecting)
            const Center(child: CircularProgressIndicator()),
          if (pairState.status == PairStatus.success)
            Center(
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  const Icon(Icons.check_circle, color: Colors.green, size: 64),
                  const SizedBox(height: 16),
                  const Text('Mac paired successfully!'),
                  const SizedBox(height: 16),
                  ElevatedButton(
                    onPressed: () {
                      // Home lists paired Macs from storage: reload it.
                      ref.invalidate(pairedDevicesProvider);
                      context.pop();
                    },
                    child: const Text('Done'),
                  ),
                ],
              ),
            ),
          if (pairState.status == PairStatus.error)
            Center(
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  const Icon(Icons.error, color: Colors.red, size: 64),
                  const SizedBox(height: 16),
                  Padding(
                    padding: const EdgeInsets.symmetric(horizontal: 24),
                    child: SelectableText(
                      pairState.error ?? 'Unknown error',
                      textAlign: TextAlign.center,
                    ),
                  ),
                  const SizedBox(height: 16),
                  TextButton.icon(
                    icon: const Icon(Icons.copy),
                    label: const Text('Copy error'),
                    onPressed: () {
                      Clipboard.setData(
                        ClipboardData(text: pairState.error ?? ''),
                      );
                      ScaffoldMessenger.of(context).showSnackBar(
                        const SnackBar(content: Text('Error copied')),
                      );
                    },
                  ),
                  ElevatedButton(
                    onPressed: () {
                      _scanned = false;
                      ref.invalidate(pairNotifierProvider);
                    },
                    child: const Text('Try Again'),
                  ),
                ],
              ),
            ),
        ],
      ),
    );
  }
}
