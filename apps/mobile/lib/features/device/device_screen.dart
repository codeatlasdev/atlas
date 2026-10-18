import 'package:flutter/material.dart';
import 'package:go_router/go_router.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/features/projects/projects_screen.dart';

/// A connected Mac: its projects. Agents live inside each project (by terminal folder).
class DeviceScreen extends StatelessWidget {
  const DeviceScreen({super.key, required this.device});

  final MacDevice device;

  @override
  Widget build(BuildContext context) => Scaffold(
    appBar: AppBar(
      leading: BackButton(onPressed: () => context.pop()),
      title: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(device.name),
          Text('Projects', style: Theme.of(context).textTheme.bodySmall),
        ],
      ),
    ),
    body: ProjectsScreen(device: device),
  );
}
