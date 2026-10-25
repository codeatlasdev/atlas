import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/agent_logo.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/agents/agents_provider.dart';

/// A freshly launched agent: its pane key (and the agent id it runs).
typedef LaunchedAgent = ({String pane, String agent});

/// Pick an installed agent, optionally write its first prompt, launch it in [projectId].
/// Pops with the launched agent; a failed launch stays open with the error inline.
Future<LaunchedAgent?> showAgentLauncher(
  BuildContext context, {
  required MacDevice device,
  required String projectId,
}) => showModalBottomSheet<LaunchedAgent>(
  context: context,
  isScrollControlled: true,
  showDragHandle: true,
  backgroundColor: AtlasColors.of(context).overlay,
  builder: (_) => _Launcher(device: device, projectId: projectId),
);

class _Launcher extends ConsumerStatefulWidget {
  const _Launcher({required this.device, required this.projectId});
  final MacDevice device;
  final String projectId;

  @override
  ConsumerState<_Launcher> createState() => _LauncherState();
}

class _LauncherState extends ConsumerState<_Launcher> {
  final _prompt = TextEditingController();
  String? _agent;
  bool _busy = false;
  String? _error;

  @override
  void dispose() {
    _prompt.dispose();
    super.dispose();
  }

  Future<void> _launch(String agent) async {
    final mux = ref.read(connectionNotifierProvider(widget.device)).value;
    if (mux == null) {
      setState(() => _error = 'Not connected to your computer');
      return;
    }
    HapticFeedback.lightImpact();
    setState(() {
      _busy = true;
      _error = null;
    });
    final text = _prompt.text.trim();
    final resp = await mux.call('agent_launch', {
      'project': widget.projectId,
      'agent': agent,
      if (text.isNotEmpty) 'prompt': text,
    });
    if (!mounted) return;
    final data = resp['data'];
    final pane = resp['ok'] == true && data is Map
        ? data['pane'] as String?
        : null;
    if (pane == null) {
      setState(() {
        _busy = false;
        _error = resp['error'] as String? ?? 'Could not launch the agent';
      });
      return;
    }
    Navigator.pop(context, (pane: pane, agent: agent));
  }

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final installed = ref.watch(installedAgentsProvider(widget.device));
    final list = installed.value;
    // The agent to start: the user's pick, else the first that reports to Atlas.
    final selected =
        _agent ?? (list?.isNotEmpty == true ? list!.first.id : null);
    return Padding(
      padding: EdgeInsets.only(bottom: MediaQuery.viewInsetsOf(context).bottom),
      child: SafeArea(
        child: SingleChildScrollView(
          padding: const EdgeInsets.fromLTRB(Space.lg, 0, Space.lg, Space.lg),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Text('New agent', style: Theme.of(context).textTheme.titleMedium),
              const SizedBox(height: Space.md),
              if (list == null && installed.isLoading)
                const Padding(
                  padding: EdgeInsets.all(Space.lg),
                  child: Center(child: CircularProgressIndicator()),
                )
              else if (list == null || list.isEmpty)
                Padding(
                  padding: const EdgeInsets.symmetric(vertical: Space.lg),
                  child: Text(
                    'No agent found on your computer. Install omp, Claude Code or '
                    'Codex there first.',
                    style: TextStyle(color: c.muted, height: 1.4),
                  ),
                )
              else ...[
                Wrap(
                  spacing: Space.sm,
                  runSpacing: Space.sm,
                  children: [
                    for (final a in list)
                      _AgentChoice(
                        agent: a,
                        selected: a.id == selected,
                        onTap: _busy
                            ? null
                            : () => setState(() => _agent = a.id),
                      ),
                  ],
                ),
                const SizedBox(height: Space.md),
                TextField(
                  controller: _prompt,
                  minLines: 3,
                  maxLines: 6,
                  enabled: !_busy,
                  textCapitalization: TextCapitalization.sentences,
                  decoration: InputDecoration(
                    hintText: 'First prompt (optional)',
                    filled: true,
                    fillColor: c.ground,
                    border: OutlineInputBorder(
                      borderRadius: BorderRadius.circular(Space.radius),
                      borderSide: BorderSide.none,
                    ),
                  ),
                ),
              ],
              if (_error != null)
                Padding(
                  padding: const EdgeInsets.only(top: Space.md),
                  child: Text(
                    _error!,
                    style: TextStyle(color: c.error, fontSize: 13),
                  ),
                ),
              const SizedBox(height: Space.lg),
              FilledButton.icon(
                onPressed: _busy || selected == null
                    ? null
                    : () => _launch(selected),
                icon: _busy
                    ? const SizedBox.square(
                        dimension: 16,
                        child: CircularProgressIndicator(strokeWidth: 2),
                      )
                    : const Icon(Icons.rocket_launch_outlined, size: 18),
                label: Text(_busy ? 'Starting…' : 'Start agent'),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _AgentChoice extends StatelessWidget {
  const _AgentChoice({
    required this.agent,
    required this.selected,
    required this.onTap,
  });

  final AgentIntegration agent;
  final bool selected;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final ok = agent.state == 'ok';
    return Semantics(
      button: true,
      selected: selected,
      label: agent.label,
      child: InkWell(
        borderRadius: BorderRadius.circular(Space.radius),
        onTap: onTap,
        child: AnimatedContainer(
          duration: Motion.fast,
          padding: const EdgeInsets.symmetric(
            horizontal: Space.md,
            vertical: Space.sm + 2,
          ),
          decoration: BoxDecoration(
            color: selected ? c.accent.withValues(alpha: .14) : c.base,
            border: Border.all(color: selected ? c.accent : c.borderSubtle),
            borderRadius: BorderRadius.circular(Space.radius),
          ),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              AgentLogo(agent.id, size: 28),
              const SizedBox(width: Space.sm + 2),
              Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    agent.label,
                    style: const TextStyle(fontWeight: FontWeight.w600),
                  ),
                  if (!ok)
                    Text(
                      'hooks off',
                      style: TextStyle(fontSize: 11, color: c.warning),
                    ),
                ],
              ),
            ],
          ),
        ),
      ),
    );
  }
}
