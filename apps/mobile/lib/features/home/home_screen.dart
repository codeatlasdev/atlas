import 'package:go_router/go_router.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/connection/connection_notifier.dart';

/// Provides the list of paired [MacDevice]s loaded from secure storage.
final pairedDevicesProvider = FutureProvider.autoDispose<List<MacDevice>>(
  (ref) => ref.read(macStorageProvider).loadDevices(),
);

/// Home screen: shows all paired Macs and their connection states.
class HomeScreen extends ConsumerWidget {
  const HomeScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final devicesAsync = ref.watch(pairedDevicesProvider);
    return Scaffold(
      appBar: AppBar(
        title: const Text('Atlas Mobile'),
        actions: [
          IconButton(
            icon: const Icon(Icons.add),
            tooltip: 'Pair a Mac',
            onPressed: () => context.push('/pair'),
          ),
        ],
      ),
      body: devicesAsync.when(
        loading: () => const Center(child: CircularProgressIndicator()),
        error: (e, _) => Center(child: Text('Error: $e')),
        data: (devices) {
          if (devices.isEmpty) {
            return const Center(
              child: Text('No Macs paired yet.\nTap + to pair one.'),
            );
          }
          return ListView.builder(
            itemCount: devices.length,
            itemBuilder: (context, i) {
              final device = devices[i];
              return _MacTile(device: device);
            },
          );
        },
      ),
    );
  }
}

class _MacTile extends ConsumerWidget {
  const _MacTile({required this.device});
  final MacDevice device;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final muxAsync = ref.watch(connectionNotifierProvider(device));
    final isConnected = muxAsync.value != null;
    return ListTile(
      leading: Icon(
        Icons.computer,
        color: isConnected ? Colors.green : Colors.grey,
      ),
      title: Text(device.name),
      subtitle: Text(
        isConnected
            ? 'Connected'
            : muxAsync.isLoading
            ? 'Connecting…'
            : 'Disconnected',
      ),
      onTap: isConnected ? () => context.push('/device', extra: device) : null,
    );
  }
}
