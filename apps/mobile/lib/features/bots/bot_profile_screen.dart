import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/features/bots/bots_provider.dart';

// ── screen ────────────────────────────────────────────────────────────────────

class BotProfileScreen extends ConsumerStatefulWidget {
  const BotProfileScreen({
    super.key,
    required this.device,
    required this.botId,
  });

  final MacDevice device;
  final String botId;

  @override
  ConsumerState<BotProfileScreen> createState() => _BotProfileScreenState();
}

class _BotProfileScreenState extends ConsumerState<BotProfileScreen> {
  List<BotRoutine> _routines = const [];
  bool _routinesLoaded = false;

  int? _memoryLines;
  bool _memoryLoaded = false;

  ShipGithubStatus? _githubStatus;
  bool _githubLoading = false;
  ShipLoginFlow? _githubFlow;
  bool _githubPolling = false;
  Timer? _pollTimer;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) => _loadDetails());
  }

  Future<void> _loadDetails() async {
    final mux = await ref.read(connectionNotifierProvider(widget.device).future);
    if (mux == null) return;

    // Load routines + memory line count from bot_get.
    final resp = await mux.call('bot_get', {'id': widget.botId});
    if (!mounted || resp['ok'] != true) return;
    final data = (resp['data'] as Map?)?.cast<String, dynamic>() ?? {};
    final rawRoutines = (data['routines'] as List?) ?? [];
    final memText = data['memory'] as String?;
    setState(() {
      _routines = [
        for (final r in rawRoutines)
          BotRoutine.fromJson((r as Map).cast<String, dynamic>()),
      ];
      _routinesLoaded = true;
      _memoryLines = memText?.split('\n').length ?? 0;
      _memoryLoaded = true;
    });

    // Load GitHub status.
    final ghResp = await mux.call('bot_github_status', {});
    if (mounted && ghResp['ok'] == true) {
      final ghData = (ghResp['data'] as Map?)?.cast<String, dynamic>() ?? {};
      setState(() {
        _githubStatus = ShipGithubStatus.fromJson(ghData);
      });
    }
  }

  Future<void> _saveRoutines(List<BotRoutine> updated) async {
    final mux = await ref.read(connectionNotifierProvider(widget.device).future);
    if (mux == null) return;
    await mux.call('bot_routine_set', {
      'id': widget.botId,
      'routines': updated.map((r) => r.toJson()).toList(),
    });
    setState(() => _routines = updated);
  }

  Future<void> _editRole(String current) async {
    final controller = TextEditingController(text: current);
    final result = await showDialog<String>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('What it does'),
        content: TextField(
          controller: controller,
          maxLines: 4,
          autofocus: true,
          decoration: const InputDecoration(
            hintText: 'Describe what this bot does…',
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(ctx, controller.text.trim()),
            child: const Text('Save'),
          ),
        ],
      ),
    );
    controller.dispose();
    if (result == null || !mounted) return;
    final mux = await ref.read(connectionNotifierProvider(widget.device).future);
    if (mux == null) return;
    await mux.call('bot_update', {'id': widget.botId, 'role': result});
    ref.invalidate(botsProvider(widget.device));
  }

  Future<void> _editMemory() async {
    final mux = await ref.read(connectionNotifierProvider(widget.device).future);
    if (mux == null) return;

    // Load full memory text.
    final resp = await mux.call('bot_memory', {'id': widget.botId});
    if (!mounted || resp['ok'] != true) return;
    final text = (resp['data'] as Map?)?['text'] as String? ?? '';
    final controller = TextEditingController(text: text);

    final saved = await showDialog<String>(
      context: context,
      builder: (ctx) => Dialog.fullscreen(
        child: Scaffold(
          appBar: AppBar(
            title: const Text('Memory'),
            leading: IconButton(
              icon: const Icon(Icons.close),
              onPressed: () => Navigator.pop(ctx),
            ),
            actions: [
              FilledButton(
                onPressed: () =>
                    Navigator.pop(ctx, controller.text),
                child: const Text('Save'),
              ),
              const SizedBox(width: Space.md),
            ],
          ),
          body: Padding(
            padding: const EdgeInsets.all(Space.lg),
            child: TextField(
              controller: controller,
              maxLines: null,
              expands: true,
              decoration: const InputDecoration(
                border: InputBorder.none,
                hintText: 'Memory contents…',
              ),
            ),
          ),
        ),
      ),
    );
    controller.dispose();
    if (saved == null || !mounted) return;
    final saveResp = await mux.call('bot_memory_set', {
      'id': widget.botId,
      'text': saved,
    });
    if (!mounted) return;
    if (saveResp['ok'] == true) {
      setState(() => _memoryLines = saved.split('\n').length);
    }
  }

  Future<void> _deleteBot() async {
    final c = AtlasColors.of(context);
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('Delete bot?'),
        content: const Text(
          'This will permanently remove the bot and its history. This cannot be undone.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx, false),
            child: const Text('Cancel'),
          ),
          TextButton(
            onPressed: () => Navigator.pop(ctx, true),
            style: TextButton.styleFrom(foregroundColor: c.error),
            child: const Text('Delete'),
          ),
        ],
      ),
    );
    if (confirmed != true || !mounted) return;
    final mux = await ref.read(connectionNotifierProvider(widget.device).future);
    if (mux == null) return;
    final resp = await mux.call('bot_delete', {'id': widget.botId});
    if (!mounted) return;
    if (resp['ok'] == true) {
      Navigator.of(context).pop();
      ref.invalidate(botsProvider(widget.device));
    } else {
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text(resp['error'] as String? ?? 'Could not delete bot'),
        ),
      );
    }
  }

  Future<void> _addRoutine() async {
    final result = await showDialog<BotRoutine>(
      context: context,
      builder: (ctx) => const _NewRoutineDialog(),
    );
    if (result == null) return;
    await _saveRoutines([..._routines, result]);
  }

  Future<void> _startLogin() async {
    final mux = await ref.read(connectionNotifierProvider(widget.device).future);
    if (mux == null) return;
    setState(() => _githubLoading = true);
    try {
      final resp = await mux.call('bot_github_login_start', {});
      if (!mounted) return;
      if (resp['ok'] != true) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text(resp['error']?.toString() ?? 'Failed to start login')),
        );
        return;
      }
      final data = (resp['data'] as Map?)?.cast<String, dynamic>() ?? {};
      setState(() {
        _githubFlow = ShipLoginFlow.fromJson(data);
        _githubPolling = true;
      });
      _startPollingTimer();
    } finally {
      if (mounted) setState(() => _githubLoading = false);
    }
  }

  void _startPollingTimer() {
    _pollTimer?.cancel();
    _pollTimer = Timer.periodic(const Duration(seconds: 4), (_) => _pollLogin());
  }

  Future<void> _pollLogin() async {
    final mux = ref.read(connectionNotifierProvider(widget.device)).value;
    if (mux == null) return;
    final resp = await mux.call('bot_github_login_poll', {});
    if (!mounted) return;
    final data = (resp['data'] as Map?)?.cast<String, dynamic>() ?? {};
    final state = data['state'] as String? ?? 'pending';
    if (state == 'done') {
      _stopPolling();
      final login = data['login'] as String?;
      setState(() {
        _githubFlow = null;
        _githubStatus = ShipGithubStatus(installed: true, login: login);
      });
    } else if (state == 'error') {
      _stopPolling();
      setState(() => _githubFlow = null);
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text(data['error']?.toString() ?? 'GitHub sign-in failed')),
        );
      }
    }
  }

  void _stopPolling() {
    _pollTimer?.cancel();
    _pollTimer = null;
    setState(() => _githubPolling = false);
  }

  Future<void> _logout() async {
    final mux = await ref.read(connectionNotifierProvider(widget.device).future);
    if (mux == null) return;
    await mux.call('bot_github_logout', {});
    if (!mounted) return;
    setState(() {
      _githubStatus = ShipGithubStatus(
        installed: _githubStatus?.installed ?? true,
        login: null,
      );
    });
  }

  @override
  void dispose() {
    _pollTimer?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final botsData = ref.watch(botsProvider(widget.device)).value;
    final bot = botsData?.bots.where((b) => b.id == widget.botId).firstOrNull;

    return Scaffold(
      appBar: AppBar(title: const Text('Bot profile')),
      body: bot == null
          ? Center(
              child: botsData == null
                  ? const CircularProgressIndicator()
                  : Text(
                      'Bot not found',
                      style: TextStyle(color: c.faint),
                    ),
            )
          : ListView(
              padding: const EdgeInsets.all(Space.lg),
              children: [
                // ── header ────────────────────────────────────────────────
                _ProfileHeader(
                  bot: bot,
                  hostName: botsData?.hostName ?? widget.device.name,
                ),
                const SizedBox(height: Space.xl),

                // ── what it does ──────────────────────────────────────────
                _Section(
                  title: 'What it does',
                  trailing: TextButton(
                    onPressed: () => _editRole(bot.role),
                    style: TextButton.styleFrom(
                      padding: EdgeInsets.zero,
                      minimumSize: Size.zero,
                      tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                    ),
                    child: const Text('Edit'),
                  ),
                  child: Text(
                    bot.role.isNotEmpty
                        ? bot.role
                        : 'No description yet.',
                    style: TextStyle(color: c.muted, height: 1.4),
                  ),
                ),
                const SizedBox(height: Space.lg),

                // ── approval ──────────────────────────────────────────────
                _Section(
                  title: 'Approval',
                  child: _ApprovalPicker(
                    value: bot.approval,
                    enabled: bot.approvalSupported,
                    onChanged: (val) async {
                      final mux = ref
                          .read(connectionNotifierProvider(widget.device))
                          .value;
                      if (mux == null) return;
                      await mux.call('bot_update', {
                        'id': widget.botId,
                        'approval': val,
                      });
                      ref.invalidate(botsProvider(widget.device));
                    },
                  ),
                ),
                const SizedBox(height: Space.lg),

                // ── short replies ──────────────────────────────────────────
                _Section(
                  title: 'Short replies',
                  child: _ShortRepliesRow(
                    value: bot.shortReplies,
                    onChanged: (val) async {
                      final mux = ref
                          .read(connectionNotifierProvider(widget.device))
                          .value;
                      if (mux == null) return;
                      await mux.call('bot_update', {
                        'id': widget.botId,
                        'short_replies': val,
                      });
                      ref.invalidate(botsProvider(widget.device));
                    },
                  ),
                ),
                const SizedBox(height: Space.lg),

                // ── routines ──────────────────────────────────────────────
                _Section(
                  title: 'Routines',
                  child: Column(
                    children: [
                      if (!_routinesLoaded)
                        const Center(
                          child: Padding(
                            padding: EdgeInsets.all(Space.md),
                            child: CircularProgressIndicator(),
                          ),
                        )
                      else ...[
                        for (final routine in _routines)
                          _RoutineRow(
                            routine: routine,
                            onToggle: (enabled) async {
                              final updated = _routines.map((r) {
                                if (r.id != routine.id) return r;
                                return BotRoutine(
                                  id: r.id,
                                  cron: r.cron,
                                  prompt: r.prompt,
                                  enabled: enabled,
                                );
                              }).toList();
                              await _saveRoutines(updated);
                            },
                            onDelete: () async {
                              final updated = _routines
                                  .where((r) => r.id != routine.id)
                                  .toList();
                              await _saveRoutines(updated);
                            },
                          ),
                        const SizedBox(height: Space.sm),
                        OutlinedButton.icon(
                          onPressed: _addRoutine,
                          icon: const Icon(Icons.add, size: 16),
                          label: const Text('New routine'),
                        ),
                      ],
                    ],
                  ),
                ),
                const SizedBox(height: Space.lg),

                // ── memory ────────────────────────────────────────────────
                _Section(
                  title: 'Memory',
                  child: Row(
                    children: [
                      Expanded(
                        child: Text(
                          _memoryLoaded
                              ? (_memoryLines == 0
                                  ? 'No memory stored yet'
                                  : '$_memoryLines ${_memoryLines == 1 ? 'line' : 'lines'}')
                              : '…',
                          style: TextStyle(color: c.muted),
                        ),
                      ),
                      TextButton(
                        onPressed: _editMemory,
                        child: const Text('View / Edit'),
                      ),
                    ],
                  ),
                ),
                const SizedBox(height: Space.lg),

                // ── github ────────────────────────────────────────────────
                _GithubCard(
                  status: _githubStatus,
                  flow: _githubFlow,
                  loading: _githubLoading,
                  polling: _githubPolling,
                  onConnect: _startLogin,
                  onDisconnect: _logout,
                  onCancelFlow: () {
                    _stopPolling();
                    setState(() => _githubFlow = null);
                  },
                ),

                // ── delete ────────────────────────────────────────────────
                Center(
                  child: TextButton(
                    onPressed: _deleteBot,
                    style: TextButton.styleFrom(foregroundColor: c.error),
                    child: const Text('Delete bot'),
                  ),
                ),
                const SizedBox(height: Space.lg),
              ],
            ),
    );
  }
}

// ── profile header ────────────────────────────────────────────────────────────

class _ProfileHeader extends StatelessWidget {
  const _ProfileHeader({required this.bot, required this.hostName});
  final BotListEntry bot;
  final String hostName;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Column(
      children: [
        CircleAvatar(
          radius: 32,
          backgroundColor: c.accent.withValues(alpha: .18),
          child: Text(
            bot.name.isNotEmpty ? bot.name[0].toUpperCase() : '?',
            style: TextStyle(
              color: c.accent,
              fontSize: 28,
              fontWeight: FontWeight.w700,
            ),
          ),
        ),
        const SizedBox(height: Space.md),
        Text(
          bot.name,
          style: Theme.of(context).textTheme.titleLarge,
          textAlign: TextAlign.center,
        ),
        const SizedBox(height: 4),
        Text(
          '${bot.agent} · $hostName',
          style: TextStyle(color: c.muted, fontSize: 13),
          textAlign: TextAlign.center,
        ),
      ],
    );
  }
}

// ── section wrapper ───────────────────────────────────────────────────────────

class _Section extends StatelessWidget {
  const _Section({required this.title, required this.child, this.trailing});
  final String title;
  final Widget child;
  final Widget? trailing;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        SectionHeader(title, trailing: trailing),
        child,
      ],
    );
  }
}

// ── approval picker ───────────────────────────────────────────────────────────

class _ApprovalPicker extends StatelessWidget {
  const _ApprovalPicker({
    required this.value,
    required this.onChanged,
    this.enabled = true,
  });
  final String value; // 'ask' | 'auto'
  final void Function(String) onChanged;
  final bool enabled;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        SegmentedButton<String>(
          segments: const [
            ButtonSegment(value: 'ask',  label: Text('Ask before acting')),
            ButtonSegment(value: 'auto', label: Text('Auto-approve')),
          ],
          selected: {value},
          onSelectionChanged: enabled ? (s) => onChanged(s.first) : null,
          showSelectedIcon: false,
        ),
        if (!enabled) ...[
          const SizedBox(height: 6),
          Text(
            'OpenCode bots run without approval prompts.',
            style: TextStyle(color: c.muted, fontSize: 12),
          ),
        ],
      ],
    );
  }
}

// ── routine row ───────────────────────────────────────────────────────────────

class _RoutineRow extends StatelessWidget {
  const _RoutineRow({
    required this.routine,
    required this.onToggle,
    required this.onDelete,
  });
  final BotRoutine routine;
  final void Function(bool) onToggle;
  final VoidCallback onDelete;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: Space.xs),
      child: Row(
        children: [
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  _cronLabel(routine.cron),
                  style: TextStyle(
                    fontSize: 12,
                    color: c.faint,
                    fontFamily: 'Menlo',
                    fontFamilyFallback: const [
                      'SF Mono',
                      'RobotoMono',
                      'monospace',
                    ],
                  ),
                ),
                const SizedBox(height: 2),
                Text(
                  routine.prompt,
                  style: TextStyle(color: c.text),
                  maxLines: 2,
                  overflow: TextOverflow.ellipsis,
                ),
              ],
            ),
          ),
          Switch(
            value: routine.enabled,
            onChanged: onToggle,
            activeThumbColor: c.accent,
          ),
          IconButton(
            icon: Icon(Icons.delete_outline, size: 18, color: c.faint),
            onPressed: onDelete,
            tooltip: 'Remove routine',
          ),
        ],
      ),
    );
  }
}

String _cronLabel(String cron) {
  const presets = {
    '0 8 * * *': 'Every morning 8:00',
    '0 9 * * 1-5': 'Weekdays 9:00',
  };
  return presets[cron] ?? cron;
}

// ── new routine dialog ────────────────────────────────────────────────────────

class _NewRoutineDialog extends StatefulWidget {
  const _NewRoutineDialog();

  @override
  State<_NewRoutineDialog> createState() => _NewRoutineDialogState();
}

class _NewRoutineDialogState extends State<_NewRoutineDialog> {
  final _cronCtrl = TextEditingController();
  final _promptCtrl = TextEditingController();

  static const _presets = [
    ('Every morning 8:00', '0 8 * * *'),
    ('Weekdays 9:00', '0 9 * * 1-5'),
  ];

  @override
  void dispose() {
    _cronCtrl.dispose();
    _promptCtrl.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: const Text('New routine'),
      content: SingleChildScrollView(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const Text('Schedule (cron)', style: TextStyle(fontSize: 12)),
            const SizedBox(height: Space.xs),
            TextField(
              controller: _cronCtrl,
              decoration: const InputDecoration(
                hintText: '0 8 * * *',
                isDense: true,
              ),
            ),
            const SizedBox(height: Space.xs),
            Wrap(
              spacing: Space.xs,
              children: [
                for (final p in _presets)
                  ActionChip(
                    label: Text(p.$1, style: const TextStyle(fontSize: 11)),
                    onPressed: () => setState(() => _cronCtrl.text = p.$2),
                    padding: EdgeInsets.zero,
                    visualDensity: VisualDensity.compact,
                  ),
              ],
            ),
            const SizedBox(height: Space.md),
            const Text('Prompt', style: TextStyle(fontSize: 12)),
            const SizedBox(height: Space.xs),
            TextField(
              controller: _promptCtrl,
              minLines: 2,
              maxLines: 4,
              decoration: const InputDecoration(
                hintText: 'What should the bot do?',
                isDense: true,
              ),
            ),
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('Cancel'),
        ),
        FilledButton(
          onPressed: () {
            final cron = _cronCtrl.text.trim();
            final prompt = _promptCtrl.text.trim();
            if (cron.isEmpty || prompt.isEmpty) return;
            Navigator.pop(
              context,
              BotRoutine(
                id: DateTime.now().millisecondsSinceEpoch.toString(),
                cron: cron,
                prompt: prompt,
                enabled: true,
              ),
            );
          },
          child: const Text('Add'),
        ),
      ],
    );
  }
}

// ── github card ───────────────────────────────────────────────────────────────

class _GithubCard extends StatelessWidget {
  const _GithubCard({
    required this.status,
    required this.flow,
    required this.loading,
    required this.polling,
    required this.onConnect,
    required this.onDisconnect,
    required this.onCancelFlow,
  });

  final ShipGithubStatus? status;
  final ShipLoginFlow? flow;
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
          child: Text('Connected as @$login',
              style: Theme.of(context).textTheme.bodyMedium),
        ),
        TextButton(onPressed: onDisconnect, child: const Text('Disconnect')),
      ],
    );
  }

  Widget _notConnectedView(BuildContext context) {
    return Row(
      children: [
        Expanded(
          child: Text('Not connected',
              style: Theme.of(context)
                  .textTheme
                  .bodyMedium
                  ?.copyWith(color: Theme.of(context).colorScheme.onSurfaceVariant)),
        ),
        FilledButton(
          onPressed: loading ? null : onConnect,
          child: loading
              ? const SizedBox(
                  width: 16,
                  height: 16,
                  child: CircularProgressIndicator.adaptive(strokeWidth: 2))
              : const Text('Connect'),
        ),
      ],
    );
  }

  Widget _flowView(BuildContext context, ShipLoginFlow flow) {
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
              style: theme.textTheme.headlineSmall
                  ?.copyWith(fontWeight: FontWeight.bold, letterSpacing: 2),
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
            child: Text('Waiting for authorization…',
                style: theme.textTheme.bodySmall?.copyWith(
                    color: theme.colorScheme.onSurfaceVariant)),
          ),
        TextButton(onPressed: onCancelFlow, child: const Text('Cancel')),
      ],
    );
  }
}

// ── short replies row ─────────────────────────────────────────────────────────

class _ShortRepliesRow extends StatelessWidget {
  const _ShortRepliesRow({required this.value, required this.onChanged});
  final bool? value; // null = crew default (on)
  final void Function(bool?) onChanged;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Expanded(
              child: Text(
                'Summarize long replies with a cheap model.',
                style: TextStyle(color: c.muted, fontSize: 13),
              ),
            ),
          ],
        ),
        const SizedBox(height: 8),
        SegmentedButton<String>(
          segments: const [
            ButtonSegment(value: 'crew', label: Text('Crew default')),
            ButtonSegment(value: 'on',   label: Text('On')),
            ButtonSegment(value: 'off',  label: Text('Off')),
          ],
          selected: {value == true ? 'on' : value == false ? 'off' : 'crew'},
          onSelectionChanged: (s) {
            final v = s.first == 'on' ? true : s.first == 'off' ? false : null;
            onChanged(v);
          },
        ),
      ],
    );
  }
}
