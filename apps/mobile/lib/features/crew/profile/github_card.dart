/// The profile's GitHub account card: connected, not connected, or the device code to enter.
library;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'package:atlas_mobile/core/protocol/crew_models.dart';

class GithubCard extends StatelessWidget {
  const GithubCard({
    super.key,
    required this.status,
    required this.flow,
    required this.loading,
    required this.polling,
    required this.onConnect,
    required this.onDisconnect,
    required this.onCancelFlow,
  });

  final CrewGithubStatus? status;
  final CrewGithubLogin? flow;
  final bool loading;
  final bool polling;
  final VoidCallback onConnect;
  final VoidCallback onDisconnect;
  final VoidCallback onCancelFlow;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Card(
      margin: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('GitHub', style: theme.textTheme.titleMedium),
            const SizedBox(height: 12),
            if (status == null)
              const Center(child: CircularProgressIndicator.adaptive())
            else if (flow != null)
              _flowView(context, flow!)
            else if (status!.login != null)
              _connectedView(context, status!.login!)
            else
              _notConnectedView(context),
          ],
        ),
      ),
    );
  }

  Widget _connectedView(BuildContext context, String login) {
    return Row(
      children: [
        const Icon(Icons.check_circle_outline, size: 16),
        const SizedBox(width: 8),
        Expanded(
          child: Text(
            'Connected as @$login',
            style: Theme.of(context).textTheme.bodyMedium,
          ),
        ),
        TextButton(onPressed: onDisconnect, child: const Text('Disconnect')),
      ],
    );
  }

  Widget _notConnectedView(BuildContext context) {
    return Row(
      children: [
        Expanded(
          child: Text(
            'Not connected',
            style: Theme.of(context).textTheme.bodyMedium?.copyWith(
              color: Theme.of(context).colorScheme.onSurfaceVariant,
            ),
          ),
        ),
        FilledButton(
          onPressed: loading ? null : onConnect,
          child: loading
              ? const SizedBox(
                  width: 16,
                  height: 16,
                  child: CircularProgressIndicator.adaptive(strokeWidth: 2),
                )
              : const Text('Connect'),
        ),
      ],
    );
  }

  Widget _flowView(BuildContext context, CrewGithubLogin flow) {
    final theme = Theme.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          'Go to ${flow.verificationUri} and enter this code:',
          style: theme.textTheme.bodySmall,
        ),
        const SizedBox(height: 8),
        Row(
          children: [
            Text(
              flow.userCode,
              style: theme.textTheme.headlineSmall?.copyWith(
                fontWeight: FontWeight.bold,
                letterSpacing: 2,
              ),
            ),
            const SizedBox(width: 8),
            IconButton(
              icon: const Icon(Icons.copy),
              tooltip: 'Copy code',
              onPressed: () {
                Clipboard.setData(ClipboardData(text: flow.userCode));
              },
            ),
          ],
        ),
        if (polling)
          Padding(
            padding: const EdgeInsets.only(top: 4),
            child: Text(
              'Waiting for authorization…',
              style: theme.textTheme.bodySmall?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
              ),
            ),
          ),
        TextButton(onPressed: onCancelFlow, child: const Text('Cancel')),
      ],
    );
  }
}
