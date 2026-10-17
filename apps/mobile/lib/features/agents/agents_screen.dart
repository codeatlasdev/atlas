import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/features/agents/agents_provider.dart';

/// Shows all agent panes on [device] as a tab bar.
class AgentsScreen extends ConsumerWidget {
  const AgentsScreen({super.key, required this.device});

  final MacDevice device;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final mapAsync = ref.watch(agentStatusMapNotifierProvider(device));

    return mapAsync.when(
      loading: () => const Center(child: CircularProgressIndicator()),
      error: (e, _) => Center(child: Text('Error: $e')),
      data: (map) {
        if (map.isEmpty) {
          return const Center(child: Text('No agent panes active.'));
        }
        final panes = map.keys.toList();
        return DefaultTabController(
          length: panes.length,
          child: Column(
            children: [
              TabBar(
                isScrollable: true,
                tabs: [
                  for (final key in panes)
                    Tab(text: key.contains('.') ? key.split('.').last : key),
                ],
              ),
              Expanded(
                child: TabBarView(
                  children: [
                    for (final key in panes)
                      _AgentPane(
                        device: device,
                        paneKey: key,
                        status: map[key]!,
                      ),
                  ],
                ),
              ),
            ],
          ),
        );
      },
    );
  }
}

class _AgentPane extends ConsumerStatefulWidget {
  const _AgentPane({
    required this.device,
    required this.paneKey,
    required this.status,
  });

  final MacDevice device;
  final String paneKey;
  final AgentStatus status;

  @override
  ConsumerState<_AgentPane> createState() => _AgentPaneState();
}

class _AgentPaneState extends ConsumerState<_AgentPane> {
  final TextEditingController _msgCtrl = TextEditingController();
  bool _sending = false;

  @override
  void dispose() {
    _msgCtrl.dispose();
    super.dispose();
  }

  Color _stateColor(String state) => switch (state) {
        'idle' => Colors.grey,
        'working' => Colors.blue,
        'blocked' => Colors.orange,
        'waiting' => Colors.amber,
        'done' => Colors.green,
        _ => Colors.grey,
      };

  Future<void> _sendMessage() async {
    final text = _msgCtrl.text.trim();
    if (text.isEmpty) return;
    final mux =
        ref.read(connectionNotifierProvider(widget.device)).value;
    if (mux == null) return;
    setState(() => _sending = true);
    try {
      await mux.call('agent_send', {'pane': widget.paneKey, 'text': text});
      _msgCtrl.clear();
    } finally {
      if (mounted) setState(() => _sending = false);
    }
  }

  Future<void> _approve(AgentApproval approval, bool allow) async {
    final mux =
        ref.read(connectionNotifierProvider(widget.device)).value;
    if (mux == null) return;
    // Optimistically clear the approval before the round-trip.
    ref.invalidate(
      pendingApprovalNotifierProvider((widget.device, widget.paneKey)),
    );
    await mux.call('agent_approve', {
      'pane': widget.paneKey,
      'req': approval.req,
      'allow': allow,
    });
  }

  @override
  Widget build(BuildContext context) {
    final status = widget.status;
    final approval = ref.watch(
      pendingApprovalNotifierProvider((widget.device, widget.paneKey)),
    );
    final canSend = canSendChat(status) && !_sending;
    return Column(
      children: [
        // ── State chip + meta ────────────────────────────────────────────────
        Padding(
          padding: const EdgeInsets.all(12),
          child: Row(
            children: [
              Chip(
                avatar: CircleAvatar(
                  backgroundColor: _stateColor(status.state),
                  radius: 6,
                  child: const SizedBox.shrink(),
                ),
                label: Text(status.state),
                visualDensity: VisualDensity.compact,
              ),
              const SizedBox(width: 8),
              Expanded(
                child: Text(
                  status.agent,
                  style: Theme.of(context).textTheme.bodySmall,
                  overflow: TextOverflow.ellipsis,
                ),
              ),
            ],
          ),
        ),

        // ── Prompt + last message ────────────────────────────────────────────
        if (status.prompt != null)
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 4),
            child: Align(
              alignment: Alignment.centerLeft,
              child: Text(
                status.prompt!,
                maxLines: 2,
                overflow: TextOverflow.ellipsis,
                style: Theme.of(context)
                    .textTheme
                    .bodyMedium
                    ?.copyWith(fontStyle: FontStyle.italic),
              ),
            ),
          ),
        if (status.lastMessage != null)
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 4),
            child: Align(
              alignment: Alignment.centerLeft,
              child: Text(
                status.lastMessage!,
                maxLines: 3,
                overflow: TextOverflow.ellipsis,
                style: Theme.of(context).textTheme.bodySmall,
              ),
            ),
          ),

        const Spacer(),

        // ── Approval banner ──────────────────────────────────────────────────
        if (approval != null)
          Container(
            color: Colors.amber.shade100,
            padding: const EdgeInsets.all(12),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  'Tool: ${approval.toolName}',
                  style: const TextStyle(fontWeight: FontWeight.bold),
                ),
                const SizedBox(height: 4),
                Text(
                  approval.toolInput.length > 100
                      ? '${approval.toolInput.substring(0, 100)}…'
                      : approval.toolInput,
                  style: const TextStyle(fontFamily: 'monospace', fontSize: 12),
                ),
                const SizedBox(height: 8),
                Row(
                  mainAxisAlignment: MainAxisAlignment.end,
                  children: [
                    OutlinedButton(
                      onPressed: () => _approve(approval, false),
                      child: const Text('Deny'),
                    ),
                    const SizedBox(width: 8),
                    FilledButton(
                      onPressed: () => _approve(approval, true),
                      child: const Text('Allow'),
                    ),
                  ],
                ),
              ],
            ),
          ),

        // ── Chat input ───────────────────────────────────────────────────────
        Padding(
          padding: EdgeInsets.fromLTRB(
            12,
            8,
            12,
            MediaQuery.of(context).viewInsets.bottom + 12,
          ),
          child: Row(
            children: [
              Expanded(
                child: TextField(
                  controller: _msgCtrl,
                  decoration: const InputDecoration(
                    hintText: 'Message\u2026',
                    isDense: true,
                    border: OutlineInputBorder(),
                  ),
                  maxLines: null,
                  textInputAction: TextInputAction.send,
                  onSubmitted: canSend ? (_) => _sendMessage() : null,
                ),
              ),
              const SizedBox(width: 8),
              IconButton.filled(
                icon: _sending
                    ? const SizedBox.square(
                        dimension: 18,
                        child: CircularProgressIndicator(strokeWidth: 2),
                      )
                    : const Icon(Icons.send),
                onPressed: canSend ? _sendMessage : null,
                tooltip: canSend
                    ? 'Send'
                    : 'Agent is ${status.state} — cannot send',
              ),
            ],
          ),
        ),
      ],
    );
  }
}
