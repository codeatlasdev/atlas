import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/agent_logo.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/features/bots/automations.dart';
import 'package:atlas_mobile/features/bots/automation_editor_sheet.dart';
import 'package:atlas_mobile/features/bots/bots_provider.dart';
import 'package:atlas_mobile/features/bots/chat_motion.dart';
import 'package:atlas_mobile/features/bots/crew_text.dart';
import 'package:atlas_mobile/features/bots/crewmate.dart';

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
  ShipGithubStatus? _githubStatus;
  bool _githubLoading = false;
  ShipLoginFlow? _githubFlow;
  bool _githubPolling = false;
  Timer? _pollTimer;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) => _loadGithub());
  }

  Future<void> _loadGithub() async {
    final mux = await ref.read(
      connectionNotifierProvider(widget.device).future,
    );
    if (mux == null) return;
    final ghResp = await mux.call('bot_github_status', {});
    if (mounted && ghResp['ok'] == true) {
      final ghData = (ghResp['data'] as Map?)?.cast<String, dynamic>() ?? {};
      setState(() {
        _githubStatus = ShipGithubStatus.fromJson(ghData);
      });
    }
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
    final mux = await ref.read(
      connectionNotifierProvider(widget.device).future,
    );
    if (mux == null) return;
    await mux.call('bot_update', {'id': widget.botId, 'role': result});
    ref.invalidate(botsProvider(widget.device));
  }

  Future<void> _editMemory() async {
    final mux = await ref.read(
      connectionNotifierProvider(widget.device).future,
    );
    if (mux == null) return;

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
                onPressed: () => Navigator.pop(ctx, controller.text),
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
    await mux.call('bot_memory_set', {'id': widget.botId, 'text': saved});
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
    final mux = await ref.read(
      connectionNotifierProvider(widget.device).future,
    );
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

  // ── automations ───────────────────────────────────────────────────────────

  Future<void> _saveTriggers(List<Trigger> triggers) async {
    final mux = await ref.read(
      connectionNotifierProvider(widget.device).future,
    );
    if (mux == null) return;
    await mux.call('bot_triggers_set', {
      'id': widget.botId,
      'triggers': triggers.map((t) => t.toJson()).toList(),
    });
    ref.invalidate(botsProvider(widget.device));
  }

  Future<void> _addAutomation(String botName) async {
    final result = await showAutomationEditor(context, botName: botName);
    if (result == null) return;
    final bot = ref
        .read(botsProvider(widget.device))
        .value
        ?.bots
        .where((b) => b.id == widget.botId)
        .firstOrNull;
    await _saveTriggers([...?bot?.triggers, result]);
  }

  Future<void> _editAutomation(BotListEntry bot, Trigger trigger) async {
    final result = await showAutomationEditor(
      context,
      existing: trigger,
      botName: bot.name,
    );
    if (result == null) return;
    await _saveTriggers([
      for (final t in bot.triggers)
        if (t.id == trigger.id) result else t,
    ]);
  }

  Future<void> _deleteAutomation(BotListEntry bot, Trigger trigger) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('Remove automation?'),
        content: Text('Remove "${describeTrigger(trigger.kind)}"?'),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx, false),
            child: const Text('Cancel'),
          ),
          TextButton(
            onPressed: () => Navigator.pop(ctx, true),
            style: TextButton.styleFrom(
              foregroundColor: AtlasColors.of(context).error,
            ),
            child: const Text('Remove'),
          ),
        ],
      ),
    );
    if (confirmed != true) return;
    await _saveTriggers(bot.triggers.where((t) => t.id != trigger.id).toList());
  }

  // ── github ────────────────────────────────────────────────────────────────

  Future<void> _startLogin() async {
    final mux = await ref.read(
      connectionNotifierProvider(widget.device).future,
    );
    if (mux == null) return;
    setState(() => _githubLoading = true);
    try {
      final resp = await mux.call('bot_github_login_start', {});
      if (!mounted) return;
      if (resp['ok'] != true) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text(resp['error']?.toString() ?? 'Failed to start login'),
          ),
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
    _pollTimer = Timer.periodic(
      const Duration(seconds: 4),
      (_) => _pollLogin(),
    );
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
          SnackBar(
            content: Text(data['error']?.toString() ?? 'GitHub sign-in failed'),
          ),
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
    final mux = await ref.read(
      connectionNotifierProvider(widget.device).future,
    );
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

    final hostName = botsData?.hostName ?? widget.device.name;
    final state = bot == null ? CrewState.idle : CrewState.of(bot.state);
    return Scaffold(
      body: bot == null
          ? Center(
              child: botsData == null
                  ? const CircularProgressIndicator()
                  : Text(
                      'Crewmate not found',
                      style: TextStyle(color: c.faint),
                    ),
            )
          : Stack(
              children: [
                // The crewmate's own colour, a soft glow behind its portrait.
                Positioned(
                  top: -220,
                  left: -80,
                  right: -80,
                  height: 560,
                  child: IgnorePointer(
                    child: DecoratedBox(
                      decoration: BoxDecoration(
                        gradient: RadialGradient(
                          colors: [
                            CrewLook.from(
                              bot.look,
                              bot.id,
                            ).color.withValues(alpha: .26),
                            CrewLook.from(
                              bot.look,
                              bot.id,
                            ).color.withValues(alpha: 0),
                          ],
                        ),
                      ),
                    ),
                  ),
                ),
                ListView(
                  physics: const BouncingScrollPhysics(
                    parent: AlwaysScrollableScrollPhysics(),
                  ),
                  padding: EdgeInsets.fromLTRB(
                    Space.lg,
                    MediaQuery.paddingOf(context).top + 56,
                    Space.lg,
                    Space.xxl + MediaQuery.paddingOf(context).bottom,
                  ),
                  children: [
                    EnterFade(
                      child: _ProfileHeader(
                        bot: bot,
                        hostName: hostName,
                        state: state,
                      ),
                    ),
                    const SizedBox(height: Space.xl),

                    EnterFade(
                      index: 1,
                      child: _Group(
                        title: 'What it does',
                        children: [
                          _Row(
                            onTap: () => _editRole(bot.role),
                            child: Text(
                              bot.role.isNotEmpty
                                  ? bot.role
                                  : 'Tap to describe its job.',
                              style: TextStyle(
                                color: bot.role.isNotEmpty ? c.text : c.faint,
                                height: 1.45,
                                fontSize: 15,
                              ),
                            ),
                          ),
                        ],
                      ),
                    ),

                    EnterFade(
                      index: 2,
                      child: _Group(
                        title: 'How it works',
                        children: [
                          _Row(
                            label: 'Approval',
                            child: _ApprovalPicker(
                              value: bot.approval,
                              enabled: bot.approvalSupported,
                              onChanged: (val) async {
                                final mux = ref
                                    .read(
                                      connectionNotifierProvider(widget.device),
                                    )
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
                          _Row(
                            label: 'Short replies',
                            child: _ShortRepliesRow(
                              value: bot.shortReplies,
                              onChanged: (val) async {
                                final mux = ref
                                    .read(
                                      connectionNotifierProvider(widget.device),
                                    )
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
                          _Row(
                            icon: Icons.auto_stories_outlined,
                            label: 'Memory',
                            trailingText: 'View / Edit',
                            onTap: _editMemory,
                          ),
                        ],
                      ),
                    ),

                    EnterFade(
                      index: 3,
                      child: _AutomationsSection(
                        bot: bot,
                        now: DateTime.now(),
                        onAdd: () => _addAutomation(bot.name),
                        onEdit: (t) => _editAutomation(bot, t),
                        onDelete: (t) => _deleteAutomation(bot, t),
                        onToggle: (t, enabled) async {
                          await _saveTriggers([
                            for (final tr in bot.triggers)
                              if (tr.id == t.id)
                                tr.copyWith(enabled: enabled)
                              else
                                tr,
                          ]);
                        },
                      ),
                    ),
                    const SizedBox(height: Space.lg),

                    EnterFade(
                      index: 4,
                      child: _Group(
                        title: 'Accounts',
                        children: [
                          Padding(
                            padding: const EdgeInsets.all(Space.md),
                            child: _GithubCard(
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
                          ),
                        ],
                      ),
                    ),

                    EnterFade(
                      index: 5,
                      child: _Group(
                        children: [
                          _Row(
                            icon: Icons.delete_outline_rounded,
                            label: 'Remove from the crew',
                            danger: true,
                            onTap: _deleteBot,
                          ),
                        ],
                      ),
                    ),
                  ],
                ),
                // Floating back button, like the chat's.
                Positioned(
                  top: MediaQuery.paddingOf(context).top + 6,
                  left: Space.md,
                  child: Semantics(
                    button: true,
                    label: 'Back',
                    child: Pressable(
                      onTap: () => Navigator.of(context).maybePop(),
                      child: Container(
                        width: 44,
                        height: 44,
                        decoration: BoxDecoration(
                          color: c.overlay.withValues(alpha: .82),
                          shape: BoxShape.circle,
                          border: Border.all(color: c.border),
                        ),
                        child: Icon(
                          Icons.arrow_back_ios_new_rounded,
                          size: 18,
                          color: c.text,
                        ),
                      ),
                    ),
                  ),
                ),
              ],
            ),
    );
  }
}

// ── profile header ────────────────────────────────────────────────────────────

class _ProfileHeader extends StatelessWidget {
  const _ProfileHeader({
    required this.bot,
    required this.hostName,
    required this.state,
  });
  final BotListEntry bot;
  final String hostName;
  final CrewState state;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final tag = crewRoleTag(bot.role);
    final (word, color) = switch (state) {
      CrewState.working => ('Working', c.accent),
      CrewState.needsYou => ('Needs you', c.warning),
      CrewState.offline => ('Offline', c.faint),
      CrewState.idle => ('On deck', c.success),
    };
    return Column(
      children: [
        Crewmate(seed: bot.id, look: bot.look, state: state, size: 150),
        const SizedBox(height: Space.sm),
        Text(
          bot.name,
          style: TextStyle(
            fontSize: 28,
            fontWeight: FontWeight.w700,
            color: c.text,
          ),
          textAlign: TextAlign.center,
        ),
        if (tag.isNotEmpty) ...[
          const SizedBox(height: 2),
          Text(tag, style: TextStyle(color: c.muted, fontSize: 15)),
        ],
        const SizedBox(height: Space.md),
        Wrap(
          alignment: WrapAlignment.center,
          spacing: 8,
          runSpacing: 8,
          children: [
            _Chip(dot: color, label: word, color: color),
            _Chip(label: bot.agent, child: AgentLogo(bot.agent, size: 14)),
            _Chip(icon: Icons.dns_outlined, label: hostName),
          ],
        ),
      ],
    );
  }
}

class _Chip extends StatelessWidget {
  const _Chip({
    required this.label,
    this.icon,
    this.child,
    this.dot,
    this.color,
  });
  final String label;
  final IconData? icon;
  final Widget? child;
  final Color? dot;
  final Color? color;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 7),
      decoration: BoxDecoration(
        color: (color ?? c.text).withValues(alpha: .08),
        borderRadius: BorderRadius.circular(999),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (dot != null)
            StatusDot(
              color: dot!,
              size: 6,
              pulse: dot == c.accent || dot == c.warning,
            ),
          if (icon != null) ...[
            Icon(icon, size: 14, color: c.muted),
            const SizedBox(width: 6),
          ],
          if (child != null) ...[child!, const SizedBox(width: 6)],
          ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 180),
            child: Text(
              label,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(
                fontSize: 13,
                fontWeight: FontWeight.w600,
                color: color ?? c.muted,
              ),
            ),
          ),
        ],
      ),
    );
  }
}

/// An inset group: a title above one rounded card of rows with hairlines between them.
class _Group extends StatelessWidget {
  const _Group({required this.children, this.title});
  final String? title;
  final List<Widget> children;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Padding(
      padding: const EdgeInsets.only(bottom: Space.lg),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          if (title != null)
            Padding(
              padding: const EdgeInsets.only(left: Space.md, bottom: 8),
              child: Text(
                title!,
                style: TextStyle(
                  fontSize: 13,
                  fontWeight: FontWeight.w600,
                  color: c.muted,
                ),
              ),
            ),
          ClipRRect(
            borderRadius: BorderRadius.circular(20),
            child: ColoredBox(
              color: c.raised,
              child: Column(
                children: [
                  for (var i = 0; i < children.length; i++) ...[
                    if (i > 0)
                      Divider(
                        height: 1,
                        thickness: 1,
                        indent: Space.md,
                        color: c.borderSubtle,
                      ),
                    children[i],
                  ],
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }
}

/// One row of a [_Group]: a label with a control, or tappable text.
class _Row extends StatelessWidget {
  const _Row({
    this.label,
    this.icon,
    this.child,
    this.trailingText,
    this.onTap,
    this.danger = false,
  });
  final String? label;
  final IconData? icon;
  final Widget? child;
  final String? trailingText;
  final VoidCallback? onTap;
  final bool danger;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final fg = danger ? c.error : c.text;
    Widget body;
    if (label != null && child != null) {
      body = Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            label!,
            style: TextStyle(
              fontSize: 15,
              fontWeight: FontWeight.w600,
              color: fg,
            ),
          ),
          const SizedBox(height: 10),
          child!,
        ],
      );
    } else if (label != null) {
      body = Row(
        children: [
          if (icon != null) ...[
            Icon(icon, size: 20, color: danger ? c.error : c.muted),
            const SizedBox(width: 12),
          ],
          Expanded(
            child: Text(
              label!,
              style: TextStyle(
                fontSize: 15,
                fontWeight: FontWeight.w500,
                color: fg,
              ),
            ),
          ),
          if (trailingText != null)
            Text(
              trailingText!,
              style: TextStyle(fontSize: 14, color: c.accent),
            ),
          if (onTap != null && !danger) ...[
            const SizedBox(width: 4),
            Icon(Icons.chevron_right_rounded, color: c.faint, size: 20),
          ],
        ],
      );
    } else {
      body = Row(
        children: [
          Expanded(child: child ?? const SizedBox()),
          if (onTap != null)
            Icon(Icons.edit_outlined, size: 16, color: c.faint),
        ],
      );
    }
    return InkWell(
      onTap: onTap,
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: Space.md, vertical: 14),
        child: body,
      ),
    );
  }
}

// ── section wrapper ───────────────────────────────────────────────────────────

class _Section extends StatelessWidget {
  const _Section({required this.title, required this.child});
  final String title;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [SectionHeader(title), child],
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
  final String value;
  final void Function(String) onChanged;
  final bool enabled;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        _Segments(
          options: const [('ask', 'Ask first'), ('auto', 'Auto-approve')],
          value: value,
          onChanged: enabled ? onChanged : null,
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

// ── automations section ───────────────────────────────────────────────────────

class _AutomationsSection extends StatelessWidget {
  const _AutomationsSection({
    required this.bot,
    required this.now,
    required this.onAdd,
    required this.onEdit,
    required this.onDelete,
    required this.onToggle,
  });

  final BotListEntry bot;
  final DateTime now;
  final VoidCallback onAdd;
  final void Function(Trigger) onEdit;
  final void Function(Trigger) onDelete;
  final void Function(Trigger, bool) onToggle;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return _Section(
      title: 'Automations',
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          if (bot.triggers.isEmpty)
            Padding(
              padding: const EdgeInsets.symmetric(vertical: Space.md),
              child: Text(
                'Or just ask ${bot.name}: "every weekday at 9, review the open PRs".',
                style: TextStyle(color: c.faint, fontSize: 13, height: 1.4),
              ),
            )
          else
            for (final t in bot.triggers)
              _TriggerRow(
                trigger: t,
                now: now,
                onEdit: () => onEdit(t),
                onDelete: () => onDelete(t),
                onToggle: (v) => onToggle(t, v),
              ),
          const SizedBox(height: Space.sm),
          OutlinedButton.icon(
            onPressed: onAdd,
            icon: const Icon(Icons.add, size: 16),
            label: const Text('Add automation'),
          ),
        ],
      ),
    );
  }
}

class _TriggerRow extends StatelessWidget {
  const _TriggerRow({
    required this.trigger,
    required this.now,
    required this.onEdit,
    required this.onDelete,
    required this.onToggle,
  });

  final Trigger trigger;
  final DateTime now;
  final VoidCallback onEdit;
  final VoidCallback onDelete;
  final void Function(bool) onToggle;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final desc = describeTrigger(trigger.kind, tz: trigger.tz);
    final nextStr = trigger.nextFire != null
        ? describeNext(trigger.nextFire!, now)
        : null;

    return Dismissible(
      key: ValueKey(trigger.id),
      direction: DismissDirection.endToStart,
      background: Container(
        alignment: Alignment.centerRight,
        padding: const EdgeInsets.only(right: Space.lg),
        color: c.error.withValues(alpha: .15),
        child: Icon(Icons.delete_outline, color: c.error, size: 20),
      ),
      confirmDismiss: (_) async {
        onDelete();
        return false; // actual removal is handled via _saveTriggers
      },
      child: Padding(
        padding: const EdgeInsets.symmetric(vertical: Space.xs),
        child: Row(
          children: [
            Expanded(
              child: InkWell(
                onTap: onEdit,
                borderRadius: BorderRadius.circular(Space.radiusSm),
                child: Padding(
                  padding: const EdgeInsets.symmetric(vertical: 4),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(desc, style: TextStyle(color: c.text, fontSize: 14)),
                      if (nextStr != null) ...[
                        const SizedBox(height: 2),
                        Text(
                          'Next: $nextStr',
                          style: TextStyle(color: c.faint, fontSize: 12),
                        ),
                      ],
                    ],
                  ),
                ),
              ),
            ),
            Switch(
              value: trigger.enabled,
              onChanged: onToggle,
              activeThumbColor: c.accent,
            ),
            IconButton(
              icon: Icon(Icons.edit_outlined, size: 16, color: c.faint),
              onPressed: onEdit,
              tooltip: 'Edit',
            ),
          ],
        ),
      ),
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

// ── short replies row ─────────────────────────────────────────────────────────

class _ShortRepliesRow extends StatelessWidget {
  const _ShortRepliesRow({required this.value, required this.onChanged});
  final bool? value;
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
        _Segments(
          options: const [
            ('crew', 'Crew default'),
            ('on', 'On'),
            ('off', 'Off'),
          ],
          value: value == true
              ? 'on'
              : value == false
              ? 'off'
              : 'crew',
          onChanged: (v) => onChanged(
            v == 'on'
                ? true
                : v == 'off'
                ? false
                : null,
          ),
        ),
      ],
    );
  }
}

/// A pill-shaped choice: the selected option sits on a sliding rounded thumb. Labels never
/// wrap (Material's SegmentedButton broke "Crew default" in two next to its checkmark).
class _Segments extends StatelessWidget {
  const _Segments({required this.options, required this.value, this.onChanged});
  final List<(String, String)> options;
  final String value;
  final void Function(String)? onChanged;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final index = options
        .indexWhere((o) => o.$1 == value)
        .clamp(0, options.length - 1);
    return Opacity(
      opacity: onChanged == null ? .5 : 1,
      child: Container(
        height: 40,
        padding: const EdgeInsets.all(3),
        decoration: BoxDecoration(
          color: c.ground,
          borderRadius: BorderRadius.circular(20),
        ),
        child: LayoutBuilder(
          builder: (_, box) {
            final w = box.maxWidth / options.length;
            return Stack(
              children: [
                AnimatedPositioned(
                  duration: Motion.normal,
                  curve: Curves.easeOutCubic,
                  left: w * index,
                  width: w,
                  top: 0,
                  bottom: 0,
                  child: DecoratedBox(
                    decoration: BoxDecoration(
                      color: c.overlay,
                      borderRadius: BorderRadius.circular(17),
                    ),
                  ),
                ),
                Row(
                  children: [
                    for (final (v, label) in options)
                      Expanded(
                        child: Semantics(
                          button: true,
                          selected: v == value,
                          child: GestureDetector(
                            behavior: HitTestBehavior.opaque,
                            onTap: onChanged == null || v == value
                                ? null
                                : () {
                                    HapticFeedback.selectionClick();
                                    onChanged!(v);
                                  },
                            child: Center(
                              child: AnimatedDefaultTextStyle(
                                duration: Motion.fast,
                                style: TextStyle(
                                  fontSize: 13.5,
                                  fontWeight: v == value
                                      ? FontWeight.w600
                                      : FontWeight.w500,
                                  color: v == value ? c.text : c.muted,
                                ),
                                child: Text(
                                  label,
                                  maxLines: 1,
                                  overflow: TextOverflow.ellipsis,
                                ),
                              ),
                            ),
                          ),
                        ),
                      ),
                  ],
                ),
              ],
            );
          },
        ),
      ),
    );
  }
}
