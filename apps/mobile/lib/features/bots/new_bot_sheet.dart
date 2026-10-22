import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/bots/bots_provider.dart';

/// Bottom sheet for creating a new bot on a paired device.
class NewBotSheet extends ConsumerStatefulWidget {
  const NewBotSheet({super.key, required this.devices});
  final List<MacDevice> devices;

  @override
  ConsumerState<NewBotSheet> createState() => _NewBotSheetState();
}

class _NewBotSheetState extends ConsumerState<NewBotSheet> {
  final _nameCtrl = TextEditingController();
  final _roleCtrl = TextEditingController();
  String _agent = 'claude';
  String _approval = 'ask';
  late MacDevice _device;
  bool _submitting = false;
  String? _error;

  static const _agents = ['claude', 'codex', 'opencode', 'omp'];

  @override
  void initState() {
    super.initState();
    _device = widget.devices.first;
  }

  @override
  void dispose() {
    _nameCtrl.dispose();
    _roleCtrl.dispose();
    super.dispose();
  }

  void _onAgentChanged(String agent) {
    setState(() {
      _agent = agent;
      // omp forces auto-approval.
      if (agent == 'omp') _approval = 'auto';
    });
  }

  Future<void> _submit() async {
    final name = _nameCtrl.text.trim();
    if (name.isEmpty) {
      setState(() => _error = 'Name is required.');
      return;
    }
    setState(() {
      _submitting = true;
      _error = null;
    });
    try {
      final mux = await ref.read(connectionNotifierProvider(_device).future);
      if (mux == null) throw Exception('Not connected to ${_device.name}');
      final resp = await mux.call('bot_create', {
        'name': name,
        'role': _roleCtrl.text.trim(),
        'agent': _agent,
        'approval': _approval,
      });
      if (resp['ok'] != true) {
        throw Exception(resp['error'] ?? 'Failed to create bot');
      }
      ref.invalidate(botsProvider(_device));
      if (mounted) Navigator.of(context).pop();
    } catch (e) {
      setState(() => _error = '$e');
    } finally {
      if (mounted) setState(() => _submitting = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final isOmp = _agent == 'omp';

    return Padding(
      padding: EdgeInsets.only(
        left: 24,
        right: 24,
        top: 8,
        bottom: MediaQuery.of(context).viewInsets.bottom + 24,
      ),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(
            'New bot',
            style: Theme.of(context).textTheme.titleLarge,
          ),
          const SizedBox(height: 20),

          // Device picker (only when more than one device).
          if (widget.devices.length > 1) ...[
            Text('Computer', style: TextStyle(color: c.muted, fontSize: 13)),
            const SizedBox(height: 6),
            DropdownButtonFormField<MacDevice>(
              initialValue: _device,
              decoration: const InputDecoration(isDense: true),
              items: [
                for (final d in widget.devices)
                  DropdownMenuItem(value: d, child: Text(d.name)),
              ],
              onChanged: (d) {
                if (d != null) setState(() => _device = d);
              },
            ),
            const SizedBox(height: 16),
          ],

          // Name field.
          TextField(
            controller: _nameCtrl,
            decoration: const InputDecoration(
              labelText: 'Name',
              hintText: 'e.g. Release bot',
            ),
            textCapitalization: TextCapitalization.sentences,
          ),
          const SizedBox(height: 12),

          // Role field.
          TextField(
            controller: _roleCtrl,
            decoration: const InputDecoration(
              labelText: 'Role',
              hintText: 'Describe what this bot does…',
            ),
            minLines: 2,
            maxLines: 4,
            textCapitalization: TextCapitalization.sentences,
          ),
          const SizedBox(height: 16),

          // Agent picker.
          Text('Agent', style: TextStyle(color: c.muted, fontSize: 13)),
          const SizedBox(height: 6),
          DropdownButtonFormField<String>(
              initialValue: _agent,
            decoration: const InputDecoration(isDense: true),
            items: [
              for (final a in _agents)
                DropdownMenuItem(value: a, child: Text(a)),
            ],
            onChanged: (a) {
              if (a != null) _onAgentChanged(a);
            },
          ),
          const SizedBox(height: 16),

          // Approval mode.
          Text('Approval', style: TextStyle(color: c.muted, fontSize: 13)),
          const SizedBox(height: 6),
          SegmentedButton<String>(
            selected: {_approval},
            onSelectionChanged: isOmp
                ? null
                : (s) => setState(() => _approval = s.first),
            segments: const [
              ButtonSegment(value: 'ask', label: Text('Ask me')),
              ButtonSegment(value: 'auto', label: Text('Auto')),
            ],
          ),
          if (isOmp) ...[
            const SizedBox(height: 6),
            Text(
              'omp bots run without approval prompts.',
              style: TextStyle(color: c.muted, fontSize: 12),
            ),
          ],

          if (_error != null) ...[
            const SizedBox(height: 12),
            Text(_error!, style: TextStyle(color: c.error, fontSize: 13)),
          ],

          const SizedBox(height: 20),
          FilledButton(
            onPressed: _submitting ? null : _submit,
            child: _submitting
                ? const SizedBox(
                    width: 18,
                    height: 18,
                    child: CircularProgressIndicator(strokeWidth: 2),
                  )
                : const Text('Create bot'),
          ),
        ],
      ),
    );
  }
}
