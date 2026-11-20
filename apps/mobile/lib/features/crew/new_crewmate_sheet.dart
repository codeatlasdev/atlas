import 'package:material_ui/material_ui.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/connection/require_mux.dart';
import 'package:atlas_mobile/core/protocol/agent_registry.dart';
import 'package:atlas_mobile/core/protocol/crew_ops_models.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/protocol/permission_mode.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/l10n.dart';
import 'package:atlas_mobile/features/crew/crew_provider.dart';
import 'package:atlas_mobile/features/crew/ops/crew_ops_provider.dart';
import 'package:atlas_mobile/features/crew/permissions/mode_picker.dart';
import 'package:atlas_mobile/features/crew/permissions/mode_text.dart';

/// Opens the recruit sheet for a crewmate on one of [devices] (the crew servers).
Future<void> showNewCrewmateSheet(
  BuildContext context,
  List<MacDevice> devices,
) => showModalBottomSheet<void>(
  context: context,
  isScrollControlled: true,
  useSafeArea: true,
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

  /// What the person picked; until they pick, the crew's default mode.
  PermissionMode? _picked;
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

  void _onAgentChanged(String agent) => setState(() => _agent = agent);

  /// An engine with no approval channel (codex, opencode) always runs unattended: Bypass, fixed.
  bool get _canAsk => builtinAgents.any((a) => a.id == _agent && a.canAsk);

  /// The mode the crewmate will start in: the picked one, else the crew's default, else Ask; Bypass only while
  /// the crew allows it, and always Bypass for an engine that cannot ask.
  PermissionMode _mode(CrewSettings? crew) {
    if (!_canAsk) return PermissionMode.bypass;
    final allowBypass = crew?.allowBypass ?? true;
    final mode = _picked ?? crew?.defaultMode ?? PermissionMode.ask;
    return mode == PermissionMode.bypass && !allowBypass
        ? PermissionMode.ask
        : mode;
  }

  Future<void> _submit() async {
    final name = _nameCtrl.text.trim();
    if (name.isEmpty) {
      setState(() => _error = 'Name is required.');
      return;
    }
    final mode = _mode(ref.read(crewSettingsProvider(_device)).value);
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
        'mode': mode.wire,
      });
      if (resp['ok'] != true) {
        throw HostRefusal.fromReply(resp);
      }
      ref.invalidate(crewProvider(_device));
      if (mounted) Navigator.of(context).pop();
    } catch (e) {
      if (mounted) {
        setState(
          () => _error = e is HostRefusal && e.code != null
              ? modeFailureText(context.l10n, e)
              : errorText(e),
        );
      }
    } finally {
      if (mounted) setState(() => _submitting = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final l = context.l10n;
    final crew = ref.watch(crewSettingsProvider(_device)).value;
    final canAsk = _canAsk;

    // Scrolls: the three modes make it taller than a small phone at a large text size.
    return SingleChildScrollView(
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
              isExpanded: true,
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
            isExpanded: true,
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

          // Permission mode: starts at the crew's default; Bypass is not offered when the crew does not allow it.
          Text(
            l.mobile_crew_mode_label,
            style: Theme.of(context).textTheme.bodyMedium!
                .copyWith(color: c.muted),
          ),
          const SizedBox(height: Space.sm),
          ModePicker(
            value: _mode(crew),
            allowBypass: crew?.allowBypass ?? true,
            onChanged: canAsk ? (m) => setState(() => _picked = m) : null,
            reason: canAsk ? null : l.mobile_crew_mode_no_channel,
          ),

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
