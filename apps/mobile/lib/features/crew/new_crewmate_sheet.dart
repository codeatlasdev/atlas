import 'package:material_ui/material_ui.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/connection/require_mux.dart';
import 'package:atlas_mobile/core/protocol/agent_registry.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/crew/crew_provider.dart';

/// Opens the recruit sheet for a crewmate on one of [devices] (the crew servers).
Future<void> showNewCrewmateSheet(
  BuildContext context,
  List<MacDevice> devices,
) => showModalBottomSheet<void>(
  context: context,
  isScrollControlled: true,
  showDragHandle: true,
  builder: (_) => NewCrewmateSheet(devices: devices),
);

/// Bottom sheet for creating a new crewmate on a crew server.
class NewCrewmateSheet extends ConsumerStatefulWidget {
  const NewCrewmateSheet({super.key, required this.devices});
  final List<MacDevice> devices;

  @override
  ConsumerState<NewCrewmateSheet> createState() => _NewCrewmateSheetState();
}

class _NewCrewmateSheetState extends ConsumerState<NewCrewmateSheet> {
  final _nameCtrl = TextEditingController();
  final _roleCtrl = TextEditingController();
  String _agent = 'claude';
  String _approval = 'ask';
  late MacDevice _device;
  bool _submitting = false;
  String? _error;

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
      // opencode forces auto-approval (server rejects ask).
      if (agent == 'opencode') _approval = 'auto';
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
      final mux = await requireMuxOf(ref, _device);
      final resp = await mux.call('crewmate_create', {
        'name': name,
        'role': _roleCtrl.text.trim(),
        'agent': _agent,
        'approval': _approval,
      });
      if (resp['ok'] != true) {
        throw Exception(resp['error'] ?? 'Failed to create crewmate');
      }
      ref.invalidate(crewProvider(_device));
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
    final isOpencode = _agent == 'opencode';

    return Padding(
      padding: EdgeInsets.only(
        left: Space.xl,
        right: Space.xl,
        top: Space.sm,
        bottom: MediaQuery.of(context).viewInsets.bottom + Space.xl,
      ),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(
            'Recruit a crewmate',
            style: Theme.of(context).textTheme.titleLarge,
          ),
          const SizedBox(height: Space.xl),

          // Device picker (only when more than one device).
          if (widget.devices.length > 1) ...[
            Text(
              'Computer',
              style: Theme.of(context).textTheme.bodyMedium!
                  .copyWith(color: c.muted),
            ),
            const SizedBox(height: Space.sm),
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
            const SizedBox(height: Space.lg),
          ],

          // Name field.
          TextField(
            controller: _nameCtrl,
            decoration: const InputDecoration(
              labelText: 'Name',
              hintText: 'e.g. Release crewmate',
            ),
            textCapitalization: TextCapitalization.sentences,
          ),
          const SizedBox(height: Space.md),

          // Role field.
          TextField(
            controller: _roleCtrl,
            decoration: const InputDecoration(
              labelText: 'Role',
              hintText: 'Describe what this crewmate does…',
            ),
            minLines: 2,
            maxLines: 4,
            textCapitalization: TextCapitalization.sentences,
          ),
          const SizedBox(height: Space.lg),

          // Agent picker.
          Text(
            'Agent',
            style: Theme.of(context).textTheme.bodyMedium!
                .copyWith(color: c.muted),
          ),
          const SizedBox(height: Space.sm),
          DropdownButtonFormField<String>(
            initialValue: _agent,
            decoration: const InputDecoration(isDense: true),
            items: [
              for (final a in builtinAgents)
                DropdownMenuItem(value: a.id, child: Text(a.label)),
            ],
            onChanged: (a) {
              if (a != null) _onAgentChanged(a);
            },
          ),
          const SizedBox(height: Space.lg),

          // Approval mode.
          Text(
            'Approval',
            style: Theme.of(context).textTheme.bodyMedium!
                .copyWith(color: c.muted),
          ),
          const SizedBox(height: Space.sm),
          SegmentedButton<String>(
            selected: {_approval},
            onSelectionChanged: isOpencode
                ? null
                : (s) => setState(() => _approval = s.first),
            segments: const [
              ButtonSegment(value: 'ask', label: Text('Ask me')),
              ButtonSegment(value: 'auto', label: Text('Auto')),
            ],
          ),
          if (isOpencode) ...[
            const SizedBox(height: Space.sm),
            Text(
              'OpenCode crewmates run without approval prompts.',
              style: Theme.of(context).textTheme.bodySmall!
                  .copyWith(color: c.muted),
            ),
          ],

          if (_error != null) ...[
            const SizedBox(height: Space.md),
            Text(
              _error!,
              style: Theme.of(context).textTheme.bodyMedium!
                  .copyWith(color: c.error),
            ),
          ],

          const SizedBox(height: Space.xl),
          FilledButton(
            onPressed: _submitting ? null : _submit,
            child: _submitting
                ? const SizedBox(
                    width: Space.lg,
                    height: Space.lg,
                    child: CircularProgressIndicator(strokeWidth: 2),
                  )
                : const Text('Create crewmate'),
          ),
        ],
      ),
    );
  }
}
