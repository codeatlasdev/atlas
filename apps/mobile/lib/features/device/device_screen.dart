import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/features/projects/projects_screen.dart';
import 'package:atlas_mobile/features/agents/agents_screen.dart';

/// Tab shell for a connected Mac device.
///
/// Tab 0: Projects  Tab 1: Agents
class DeviceScreen extends ConsumerStatefulWidget {
  const DeviceScreen({super.key, required this.device});

  final MacDevice device;

  @override
  ConsumerState<DeviceScreen> createState() => _DeviceScreenState();
}

class _DeviceScreenState extends ConsumerState<DeviceScreen> {
  int _tab = 0;

  static const _tabs = [
    (label: 'Projects', icon: Icons.folder_outlined),
    (label: 'Agents', icon: Icons.smart_toy_outlined),
  ];

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: Text(widget.device.name),
        leading: BackButton(onPressed: () => context.pop()),
      ),
      body: IndexedStack(
        index: _tab,
        children: [
          ProjectsScreen(device: widget.device),
          AgentsScreen(device: widget.device),
        ],
      ),
      bottomNavigationBar: NavigationBar(
        selectedIndex: _tab,
        onDestinationSelected: (i) => setState(() => _tab = i),
        destinations: [
          for (final t in _tabs)
            NavigationDestination(icon: Icon(t.icon), label: t.label),
        ],
      ),
    );
  }
}
