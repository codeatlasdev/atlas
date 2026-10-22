import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/features/agents/approval_banner.dart';
import 'package:atlas_mobile/features/bots/bots_provider.dart';

/// Full-screen view of one bot: its task thread, pending approval, and composer.
class BotScreen extends ConsumerStatefulWidget {
  const BotScreen({super.key, required this.device, required this.botId});
  final MacDevice device;
  final String botId;

  @override
  ConsumerState<BotScreen> createState() => _BotScreenState();
}

class _BotScreenState extends ConsumerState<BotScreen> {
  final _ctrl = TextEditingController();
  final _scrollCtrl = ScrollController();
  bool _sending = false;

  @override
  void dispose() {
    _ctrl.dispose();
    _scrollCtrl.dispose();
    super.dispose();
  }

  Future<void> _send() async {
    final text = _ctrl.text.trim();
    if (text.isEmpty) return;
    setState(() => _sending = true);
    _ctrl.clear();
    try {
      final mux =
          await ref.read(connectionNotifierProvider(widget.device).future);
      if (mux == null) return;
      await mux.call('bot_task', {
        'id': widget.botId,
        'prompt': text,
        'source': 'mobile',
      });
      ref.invalidate(botTasksProvider((widget.device, widget.botId)));
      ref.invalidate(botsProvider(widget.device));
    } finally {
      if (mounted) setState(() => _sending = false);
    }
  }

  Future<void> _approve(BotPendingApproval pa, bool allow) async {
    final mux =
        await ref.read(connectionNotifierProvider(widget.device).future);
    if (mux == null) return;
    await mux.call('agent_approve', {
      'pane': pa.paneKey,
      'req': pa.req,
      'allow': allow,
    });
    ref.invalidate(botsProvider(widget.device));
  }

  @override
  Widget build(BuildContext context) {
    final botsAsync = ref.watch(botsProvider(widget.device));
    final tasksAsync =
        ref.watch(botTasksProvider((widget.device, widget.botId)));
    final c = AtlasColors.of(context);

    final bot = botsAsync.value?.where((b) => b.id == widget.botId).firstOrNull;

    return Scaffold(
      appBar: AppBar(
        title: Text(bot?.name ?? 'Bot'),
        actions: [
          if (bot != null) _BotMenu(device: widget.device, bot: bot),
        ],
      ),
      body: Column(
        children: [
          // Task thread.
          Expanded(
            child: tasksAsync.when(
              loading: () =>
                  const Center(child: CircularProgressIndicator()),
              error: (e, _) => Center(
                child: Text('$e', style: TextStyle(color: c.error)),
              ),
              data: (tasks) {
                if (tasks.isEmpty) {
                  return Center(
                    child: Text(
                      'No tasks yet. Send a prompt below.',
                      style: TextStyle(color: c.muted),
                    ),
                  );
                }
                return ListView.builder(
                  controller: _scrollCtrl,
                  reverse: true,
                  padding: const EdgeInsets.fromLTRB(
                    Space.lg,
                    Space.md,
                    Space.lg,
                    Space.sm,
                  ),
                  itemCount: tasks.length,
                  itemBuilder: (_, i) => _TaskTile(task: tasks[i]),
                );
              },
            ),
          ),

          // Pending approval banner.
          if (bot?.pendingApproval != null)
            ApprovalBanner(
              approval: AgentApproval(
                paneKey: bot!.pendingApproval!.paneKey,
                req: bot.pendingApproval!.req,
                toolName: bot.pendingApproval!.toolName,
                toolInput: bot.pendingApproval!.toolInput,
              ),
              onAnswer: (allow) => _approve(bot.pendingApproval!, allow),
            ),

          // Composer.
          _Composer(
            ctrl: _ctrl,
            sending: _sending,
            onSend: _send,
          ),
        ],
      ),
    );
  }
}

// ─── Task tile ───────────────────────────────────────────────────────────────

class _TaskTile extends StatelessWidget {
  const _TaskTile({required this.task});
  final BotTask task;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        // User prompt bubble (right-aligned).
        _Bubble(text: task.prompt, mine: true),
        const SizedBox(height: 6),
        // Assistant response or state indicator.
        if (task.result != null)
          _Bubble(text: task.result!, mine: false)
        else if (task.error != null)
          _Bubble(
            text: task.error!,
            mine: false,
            color: c.error,
          )
        else
          _StateChip(state: task.state),
        const SizedBox(height: 16),
      ],
    );
  }
}

class _StateChip extends StatelessWidget {
  const _StateChip({required this.state});
  final String state;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final (label, color) = switch (state) {
      'running' => ('running…', c.accent),
      'queued' => ('queued', c.muted),
      'cancelled' => ('cancelled', c.muted),
      _ => (state, c.muted),
    };
    return Padding(
      padding: const EdgeInsets.only(top: 2),
      child: Align(
        alignment: Alignment.centerLeft,
        child: Pill(label, color: color),
      ),
    );
  }
}

class _Bubble extends StatelessWidget {
  const _Bubble({required this.text, required this.mine, this.color});
  final String text;
  final bool mine;
  final Color? color;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    const r = Radius.circular(18);
    return Align(
      alignment: mine ? Alignment.centerRight : Alignment.centerLeft,
      child: Container(
        constraints: BoxConstraints(
          maxWidth: MediaQuery.of(context).size.width * 0.82,
        ),
        padding:
            const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
        decoration: BoxDecoration(
          color: mine
              ? c.accent.withValues(alpha: .2)
              : (color ?? c.base).withValues(
                  alpha: color != null ? .15 : 1,
                ),
          border: mine
              ? null
              : Border.all(
                  color: (color ?? c.borderSubtle).withValues(
                    alpha: color != null ? .5 : 1,
                  ),
                ),
          borderRadius: BorderRadius.only(
            topLeft: r,
            topRight: r,
            bottomLeft: mine ? r : const Radius.circular(4),
            bottomRight: mine ? const Radius.circular(4) : r,
          ),
        ),
        child: SelectableText(
          text,
          style: TextStyle(
            color: color ?? c.text,
            height: 1.4,
          ),
        ),
      ),
    );
  }
}

// ─── Composer ────────────────────────────────────────────────────────────────

class _Composer extends StatelessWidget {
  const _Composer({
    required this.ctrl,
    required this.sending,
    required this.onSend,
  });
  final TextEditingController ctrl;
  final bool sending;
  final VoidCallback onSend;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return SafeArea(
      top: false,
      child: Padding(
        padding: const EdgeInsets.fromLTRB(
          Space.lg,
          Space.sm,
          Space.lg,
          Space.md,
        ),
        child: Row(
          children: [
            Expanded(
              child: TextField(
                controller: ctrl,
                decoration: InputDecoration(
                  hintText: 'Send a task…',
                  hintStyle: TextStyle(color: c.muted),
                  isDense: true,
                  contentPadding: const EdgeInsets.symmetric(
                    horizontal: 14,
                    vertical: 10,
                  ),
                ),
                minLines: 1,
                maxLines: 5,
                textCapitalization: TextCapitalization.sentences,
                onSubmitted: (_) => onSend(),
              ),
            ),
            const SizedBox(width: Space.sm),
            IconButton.filled(
              icon: sending
                  ? const SizedBox(
                      width: 18,
                      height: 18,
                      child: CircularProgressIndicator(strokeWidth: 2),
                    )
                  : const Icon(Icons.send_rounded),
              onPressed: sending ? null : onSend,
              tooltip: 'Send',
            ),
          ],
        ),
      ),
    );
  }
}

// ─── AppBar menu ─────────────────────────────────────────────────────────────

class _BotMenu extends ConsumerWidget {
  const _BotMenu({required this.device, required this.bot});
  final MacDevice device;
  final Bot bot;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    return PopupMenuButton<_MenuAction>(
      icon: const Icon(Icons.more_vert_rounded),
      onSelected: (action) => _handle(context, ref, action),
      itemBuilder: (_) => [
        const PopupMenuItem(
          value: _MenuAction.memory,
          child: Text('Memory'),
        ),
        const PopupMenuItem(
          value: _MenuAction.routines,
          child: Text('Routines'),
        ),
        const PopupMenuItem(
          value: _MenuAction.settings,
          child: Text('Settings'),
        ),
        if (bot.state == 'working' && bot.runningTask?.pane != null)
          const PopupMenuItem(
            value: _MenuAction.terminal,
            child: Text('Terminal'),
          ),
      ],
    );
  }

  void _handle(BuildContext context, WidgetRef ref, _MenuAction action) {
    switch (action) {
      case _MenuAction.memory:
        _openMemory(context, ref);
      case _MenuAction.routines:
        _openRoutines(context, ref);
      case _MenuAction.settings:
        _openSettings(context, ref);
      case _MenuAction.terminal:
        context.push('/terminal', extra: {
          'device': device,
          'pty': bot.runningTask!.pane!,
          'title': bot.name,
          'pane': bot.runningTask!.pane,
        });
    }
  }

  Future<void> _openMemory(BuildContext context, WidgetRef ref) async {
    final mux =
        await ref.read(connectionNotifierProvider(device).future);
    if (mux == null || !context.mounted) return;
    final resp = await mux.call('bot_memory', {'id': bot.id});
    if (!context.mounted) return;
    final current = resp['ok'] == true
        ? (resp['data']['text'] as String? ?? '')
        : '';
    await showDialog<void>(
      context: context,
      builder: (ctx) => _MemoryDialog(
        device: device,
        botId: bot.id,
        initial: current,
      ),
    );
  }

  Future<void> _openRoutines(BuildContext context, WidgetRef ref) async {
    await showModalBottomSheet<void>(
      context: context,
      isScrollControlled: true,
      showDragHandle: true,
      builder: (_) => _RoutinesSheet(device: device, bot: bot),
    );
  }

  Future<void> _openSettings(BuildContext context, WidgetRef ref) async {
    await showDialog<void>(
      context: context,
      builder: (_) => _SettingsDialog(device: device, bot: bot),
    );
  }
}

enum _MenuAction { memory, routines, settings, terminal }

// ─── Memory dialog ───────────────────────────────────────────────────────────

class _MemoryDialog extends ConsumerStatefulWidget {
  const _MemoryDialog({
    required this.device,
    required this.botId,
    required this.initial,
  });
  final MacDevice device;
  final String botId;
  final String initial;

  @override
  ConsumerState<_MemoryDialog> createState() => _MemoryDialogState();
}

class _MemoryDialogState extends ConsumerState<_MemoryDialog> {
  late final TextEditingController _ctrl =
      TextEditingController(text: widget.initial);
  bool _saving = false;
  String? _error;

  @override
  void dispose() {
    _ctrl.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    setState(() {
      _saving = true;
      _error = null;
    });
    try {
      final mux =
          await ref.read(connectionNotifierProvider(widget.device).future);
      if (mux == null) throw Exception('Not connected');
      final resp = await mux.call('bot_memory_set', {
        'id': widget.botId,
        'text': _ctrl.text,
      });
      if (resp['ok'] != true) throw Exception(resp['error'] ?? 'Failed');
      if (mounted) Navigator.of(context).pop();
    } catch (e) {
      setState(() => _error = '$e');
    } finally {
      if (mounted) setState(() => _saving = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return AlertDialog(
      title: const Text('Bot memory'),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          TextField(
            controller: _ctrl,
            decoration: const InputDecoration(
              hintText: 'Persistent context for this bot…',
            ),
            minLines: 5,
            maxLines: 12,
          ),
          if (_error != null) ...[
            const SizedBox(height: 8),
            Text(_error!, style: TextStyle(color: c.error, fontSize: 13)),
          ],
        ],
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('Cancel'),
        ),
        FilledButton(
          onPressed: _saving ? null : _save,
          child: _saving
              ? const SizedBox(
                  width: 16,
                  height: 16,
                  child: CircularProgressIndicator(strokeWidth: 2),
                )
              : const Text('Save'),
        ),
      ],
    );
  }
}

// ─── Routines sheet ──────────────────────────────────────────────────────────

class _RoutinesSheet extends ConsumerStatefulWidget {
  const _RoutinesSheet({required this.device, required this.bot});
  final MacDevice device;
  final Bot bot;

  @override
  ConsumerState<_RoutinesSheet> createState() => _RoutinesSheetState();
}

class _RoutinesSheetState extends ConsumerState<_RoutinesSheet> {
  late List<Routine> _routines;
  bool _saving = false;

  // Preset cron expressions shown as quick options.
  static const _presets = <(String, String)>[
    ('Every hour', '0 * * * *'),
    ('Every day at 9 am', '0 9 * * *'),
    ('Every Monday', '0 9 * * 1'),
  ];

  @override
  void initState() {
    super.initState();
    _routines = List.of(widget.bot.routines);
  }

  Future<void> _save() async {
    setState(() => _saving = true);
    try {
      final mux =
          await ref.read(connectionNotifierProvider(widget.device).future);
      if (mux == null) return;
      await mux.call('bot_routine_set', {
        'id': widget.bot.id,
        'routines': _routines
            .map((r) => {
                  'id': r.id,
                  'cron': r.cron,
                  'prompt': r.prompt,
                  'enabled': r.enabled,
                })
            .toList(),
      });
      ref.invalidate(botsProvider(widget.device));
      if (mounted) Navigator.of(context).pop();
    } finally {
      if (mounted) setState(() => _saving = false);
    }
  }

  void _add() {
    showDialog<void>(
      context: context,
      builder: (ctx) => _AddRoutineDialog(
        presets: _presets,
        onAdd: (r) => setState(() => _routines.add(r)),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
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
          Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: [
              Text(
                'Routines',
                style: Theme.of(context).textTheme.titleLarge,
              ),
              IconButton(
                icon: const Icon(Icons.add_rounded),
                onPressed: _add,
                tooltip: 'Add routine',
              ),
            ],
          ),
          if (_routines.isEmpty)
            Padding(
              padding: const EdgeInsets.symmetric(vertical: Space.xl),
              child: Text(
                'No routines. Tap + to add one.',
                textAlign: TextAlign.center,
                style: TextStyle(color: c.muted),
              ),
            )
          else
            for (var i = 0; i < _routines.length; i++)
              _RoutineTile(
                routine: _routines[i],
                onToggle: (v) => setState(
                  () => _routines[i] = Routine(
                    id: _routines[i].id,
                    cron: _routines[i].cron,
                    prompt: _routines[i].prompt,
                    enabled: v,
                    lastRun: _routines[i].lastRun,
                  ),
                ),
                onDelete: () => setState(() => _routines.removeAt(i)),
              ),
          const SizedBox(height: 16),
          FilledButton(
            onPressed: _saving ? null : _save,
            child: _saving
                ? const SizedBox(
                    width: 16,
                    height: 16,
                    child: CircularProgressIndicator(strokeWidth: 2),
                  )
                : const Text('Save'),
          ),
        ],
      ),
    );
  }
}

class _RoutineTile extends StatelessWidget {
  const _RoutineTile({
    required this.routine,
    required this.onToggle,
    required this.onDelete,
  });
  final Routine routine;
  final ValueChanged<bool> onToggle;
  final VoidCallback onDelete;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return ListTile(
      contentPadding: EdgeInsets.zero,
      title: Text(
        routine.prompt,
        maxLines: 1,
        overflow: TextOverflow.ellipsis,
      ),
      subtitle: Text(
        routine.cron,
        style: TextStyle(
          fontFamily: monoFamily,
          fontFamilyFallback: monoFallback,
          fontSize: 12,
          color: c.muted,
        ),
      ),
      trailing: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Switch(value: routine.enabled, onChanged: onToggle),
          IconButton(
            icon: Icon(Icons.delete_outline_rounded, color: c.error),
            onPressed: onDelete,
            tooltip: 'Delete',
          ),
        ],
      ),
    );
  }
}

class _AddRoutineDialog extends StatefulWidget {
  const _AddRoutineDialog({required this.presets, required this.onAdd});
  final List<(String, String)> presets;
  final ValueChanged<Routine> onAdd;

  @override
  State<_AddRoutineDialog> createState() => _AddRoutineDialogState();
}

class _AddRoutineDialogState extends State<_AddRoutineDialog> {
  final _cronCtrl = TextEditingController();
  final _promptCtrl = TextEditingController();

  @override
  void dispose() {
    _cronCtrl.dispose();
    _promptCtrl.dispose();
    super.dispose();
  }

  void _submit() {
    final cron = _cronCtrl.text.trim();
    final prompt = _promptCtrl.text.trim();
    if (cron.isEmpty || prompt.isEmpty) return;
    widget.onAdd(
      Routine(
        id: DateTime.now().millisecondsSinceEpoch.toString(),
        cron: cron,
        prompt: prompt,
        enabled: true,
      ),
    );
    Navigator.of(context).pop();
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: const Text('Add routine'),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          TextField(
            controller: _promptCtrl,
            decoration: const InputDecoration(labelText: 'Prompt'),
            minLines: 2,
            maxLines: 4,
          ),
          const SizedBox(height: 12),
          TextField(
            controller: _cronCtrl,
            decoration: InputDecoration(
              labelText: 'Cron',
              hintText: '0 9 * * *',
              suffixIcon: PopupMenuButton<String>(
                icon: const Icon(Icons.schedule_rounded),
                tooltip: 'Presets',
                onSelected: (v) =>
                    setState(() => _cronCtrl.text = v),
                itemBuilder: (_) => [
                  for (final (label, cron) in widget.presets)
                    PopupMenuItem(value: cron, child: Text(label)),
                ],
              ),
            ),
          ),
        ],
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('Cancel'),
        ),
        FilledButton(onPressed: _submit, child: const Text('Add')),
      ],
    );
  }
}

// ─── Settings dialog ─────────────────────────────────────────────────────────

class _SettingsDialog extends ConsumerStatefulWidget {
  const _SettingsDialog({required this.device, required this.bot});
  final MacDevice device;
  final Bot bot;

  @override
  ConsumerState<_SettingsDialog> createState() => _SettingsDialogState();
}

class _SettingsDialogState extends ConsumerState<_SettingsDialog> {
  late final TextEditingController _nameCtrl =
      TextEditingController(text: widget.bot.name);
  late final TextEditingController _roleCtrl =
      TextEditingController(text: widget.bot.role);
  late String _agent = widget.bot.agent;
  late String _approval = widget.bot.approval;
  bool _saving = false;
  String? _error;

  static const _agents = ['claude', 'codex', 'opencode', 'omp'];

  @override
  void dispose() {
    _nameCtrl.dispose();
    _roleCtrl.dispose();
    super.dispose();
  }

  void _onAgentChanged(String agent) {
    setState(() {
      _agent = agent;
      if (agent == 'omp') _approval = 'auto';
    });
  }

  Future<void> _save() async {
    final name = _nameCtrl.text.trim();
    if (name.isEmpty) {
      setState(() => _error = 'Name is required.');
      return;
    }
    setState(() {
      _saving = true;
      _error = null;
    });
    try {
      final mux =
          await ref.read(connectionNotifierProvider(widget.device).future);
      if (mux == null) throw Exception('Not connected');
      final resp = await mux.call('bot_update', {
        'id': widget.bot.id,
        'name': name,
        'role': _roleCtrl.text.trim(),
        'agent': _agent,
        'approval': _approval,
      });
      if (resp['ok'] != true) throw Exception(resp['error'] ?? 'Failed');
      ref.invalidate(botsProvider(widget.device));
      if (mounted) Navigator.of(context).pop();
    } catch (e) {
      setState(() => _error = '$e');
    } finally {
      if (mounted) setState(() => _saving = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final isOmp = _agent == 'omp';

    return AlertDialog(
      title: const Text('Bot settings'),
      content: SingleChildScrollView(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            TextField(
              controller: _nameCtrl,
              decoration: const InputDecoration(labelText: 'Name'),
            ),
            const SizedBox(height: 12),
            TextField(
              controller: _roleCtrl,
              decoration: const InputDecoration(labelText: 'Role'),
              minLines: 2,
              maxLines: 4,
            ),
            const SizedBox(height: 16),
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
              const SizedBox(height: 8),
              Text(_error!, style: TextStyle(color: c.error, fontSize: 13)),
            ],
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('Cancel'),
        ),
        FilledButton(
          onPressed: _saving ? null : _save,
          child: _saving
              ? const SizedBox(
                  width: 16,
                  height: 16,
                  child: CircularProgressIndicator(strokeWidth: 2),
                )
              : const Text('Save'),
        ),
      ],
    );
  }
}
