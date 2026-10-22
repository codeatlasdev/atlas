import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/providers/push_events_provider.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/features/inbox/inbox.dart';
import 'package:atlas_mobile/features/bots/bots_provider.dart';
import 'package:atlas_mobile/features/bots/new_bot_sheet.dart';
import 'package:atlas_mobile/features/bots/invite_sheet.dart';

// ── palette for stable per-bot avatar colors ──────────────────────────────────

const _kAvatarPalette = [
  Color(0xFF26A69A), // teal
  Color(0xFFFF7043), // deep-orange
  Color(0xFF7E57C2), // purple
  Color(0xFF42A5F5), // blue
  Color(0xFF66BB6A), // green
  Color(0xFFEC407A), // pink
  Color(0xFFFFCA28), // amber
  Color(0xFF26C6DA), // cyan
];

Color _botColor(String id) {
  final hash = id.codeUnits.fold(0, (acc, u) => acc ^ u);
  return _kAvatarPalette[hash.abs() % _kAvatarPalette.length];
}

// ── time helpers ──────────────────────────────────────────────────────────────

String _timeAgo(int tsMs) {
  final diff = DateTime.now()
      .difference(DateTime.fromMillisecondsSinceEpoch(tsMs));
  if (diff.inSeconds < 60) return 'now';
  if (diff.inMinutes < 60) return '${diff.inMinutes}m';
  if (diff.inHours < 24) return '${diff.inHours}h';
  if (diff.inDays == 1) return 'yesterday';
  return '${diff.inDays}d';
}

String _previewText(BotListEntry bot) {
  if (bot.state == 'working') return 'Working…';
  return bot.last?.text ?? '';
}

// ── screen ────────────────────────────────────────────────────────────────────

class BotsScreen extends ConsumerStatefulWidget {
  const BotsScreen({super.key});
  @override
  ConsumerState<BotsScreen> createState() => _BotsScreenState();
}

class _BotsScreenState extends ConsumerState<BotsScreen> {
  final Map<String, MacDevice> _inviteDeviceMap = {};
  final Set<String> _shownInvites = {};

  void _onNewInvites(List<ComputerInvite> invites) {
    for (final invite in invites) {
      if (_shownInvites.contains(invite.url)) continue;
      final device = _inviteDeviceMap[invite.url];
      if (device == null) continue;
      _shownInvites.add(invite.url);
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (!mounted) return;
        showModalBottomSheet<void>(
          context: context,
          isScrollControlled: true,
          builder: (_) => InviteSheet(
            invite: invite,
            fromDevice: device,
            onDismiss: () {
              _shownInvites.remove(invite.url);
              ref.read(computerInvitesProvider.notifier).remove(invite.url);
            },
          ),
        ).then((_) => _shownInvites.remove(invite.url));
      });
    }
  }

  Future<void> _approve(MacDevice device, BotPendingApproval pa, bool allow) async {
    final mux = ref.read(connectionNotifierProvider(device)).value;
    if (mux == null) return;
    await mux.call('agent_approve', {
      'pane': pa.paneKey,
      'req': pa.req,
      'allow': allow,
    });
  }

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final devices = ref.watch(pairedDevicesProvider).value ?? [];

    // Listen for computer_invite pushes.
    for (final device in devices) {
      ref.listen(devicePushEventsProvider(device), (_, next) {
        next.whenData((event) {
          if (event['ev'] != 'computer_invite') return;
          final invite = ComputerInvite(
            url: event['url'] as String? ?? '',
            name: event['name'] as String? ?? '',
          );
          if (invite.url.isEmpty) return;
          _inviteDeviceMap[invite.url] = device;
          ref.read(computerInvitesProvider.notifier).add(invite);
        });
      });
    }
    ref.listen<List<ComputerInvite>>(computerInvitesProvider, (_, invites) {
      _onNewInvites(invites);
    });

    // Collect and sort all bots across devices.
    final allEntries = <({BotListEntry bot, MacDevice device, String hostName, bool online})>[];
    final botServerDevices = <MacDevice>[];
    final multipleServers = devices.length > 1;

    for (final device in devices) {
      final data = ref.watch(botsProvider(device)).value;
      final online = ref.watch(connectionNotifierProvider(device)).value != null;
      if (data != null) {
        if (data.isBotServer) botServerDevices.add(device);
        for (final bot in data.bots) {
          allEntries.add((bot: bot, device: device, hostName: data.hostName, online: online));
        }
      }
    }

    // Sort: needs_you first, then by last.ts desc.
    allEntries.sort((a, b) {
      final ra = a.bot.state == 'needs_you' ? 0 : a.bot.state == 'working' ? 1 : 2;
      final rb = b.bot.state == 'needs_you' ? 0 : b.bot.state == 'working' ? 1 : 2;
      if (ra != rb) return ra.compareTo(rb);
      final ta = a.bot.last?.ts ?? 0;
      final tb = b.bot.last?.ts ?? 0;
      return tb.compareTo(ta);
    });

    final needsYou = allEntries.where((e) => e.bot.state == 'needs_you').toList();

    return Scaffold(
      appBar: AppBar(
        title: const Text('Bots', style: TextStyle(fontWeight: FontWeight.w700)),
        actions: [
          IconButton(
            icon: const Icon(Icons.add),
            tooltip: 'New bot',
            onPressed: botServerDevices.isEmpty
                ? null
                : () => showModalBottomSheet<void>(
                    context: context,
                    isScrollControlled: true,
                    showDragHandle: true,
                    builder: (_) => NewBotSheet(devices: botServerDevices),
                  ),
          ),
        ],
      ),
      body: allEntries.isEmpty
          ? EmptyState(
              icon: Icon(Icons.smart_toy_outlined, size: 64, color: c.faint),
              title: 'No bots yet',
              message: 'Run atlas bot install on a server,\nthen add it in Atlas Desktop.',
            )
          : ListView(
              children: [
                // ── Needs you section ─────────────────────────────────────
                if (needsYou.isNotEmpty) ...[
                  Padding(
                    padding: const EdgeInsets.fromLTRB(16, 16, 16, 6),
                    child: Text(
                      'Needs you',
                      style: TextStyle(
                        fontSize: 12,
                        fontWeight: FontWeight.w600,
                        color: c.warning,
                        letterSpacing: 0.5,
                      ),
                    ),
                  ),
                  for (final e in needsYou)
                    _NeedsYouCard(
                      entry: e,
                      onAnswer: (allow) => _approve(e.device, e.bot.pendingApproval!, allow),
                      onTap: () => _openBot(context, e),
                    ),
                  const SizedBox(height: 8),
                ],
                // ── Flat bot list ──────────────────────────────────────────
                for (final e in allEntries)
                  _BotRow(
                    entry: e,
                    showServer: multipleServers,
                    onTap: () => _openBot(context, e),
                  ),
                // ── New bot row ────────────────────────────────────────────
                if (botServerDevices.isNotEmpty)
                  ListTile(
                    leading: CircleAvatar(
                      radius: 22,
                      backgroundColor: c.raised,
                      child: Icon(Icons.add, color: c.accent, size: 20),
                    ),
                    title: Text('New bot', style: TextStyle(color: c.accent)),
                    onTap: () => showModalBottomSheet<void>(
                      context: context,
                      isScrollControlled: true,
                      showDragHandle: true,
                      builder: (_) => NewBotSheet(devices: botServerDevices),
                    ),
                  ),
                const SizedBox(height: 24),
              ],
            ),
    );
  }

  void _openBot(BuildContext context, ({BotListEntry bot, MacDevice device, String hostName, bool online}) e) {
    context.push('/bots/${e.device.nodeId}/${e.bot.id}', extra: {'device': e.device});
  }
}

// ── Needs-you card ────────────────────────────────────────────────────────────

class _NeedsYouCard extends StatelessWidget {
  const _NeedsYouCard({
    required this.entry,
    required this.onAnswer,
    required this.onTap,
  });

  final ({BotListEntry bot, MacDevice device, String hostName, bool online}) entry;
  final void Function(bool allow) onAnswer;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final pa = entry.bot.pendingApproval;
    final toolName = pa?.toolName ?? 'a tool';
    final cmd = pa?.toolInput is Map && (pa!.toolInput as Map)['command'] is String
        ? pa.toolInput['command'] as String
        : null;

    return InkWell(
      onTap: onTap,
      child: Container(
        margin: const EdgeInsets.symmetric(horizontal: 12, vertical: 3),
        padding: const EdgeInsets.all(14),
        decoration: BoxDecoration(
          color: c.warning.withValues(alpha: .08),
          border: Border.all(color: c.warning.withValues(alpha: .35)),
          borderRadius: BorderRadius.circular(14),
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            // Bot name row
            Row(
              children: [
                _Avatar(id: entry.bot.id, name: entry.bot.name, size: 18),
                const SizedBox(width: 8),
                Text(
                  entry.bot.name,
                  style: TextStyle(
                    fontWeight: FontWeight.w700,
                    fontSize: 14,
                    color: c.text,
                  ),
                ),
              ],
            ),
            const SizedBox(height: 8),
            // Tool line
            Row(
              children: [
                Icon(Icons.shield_outlined, size: 14, color: c.warning),
                const SizedBox(width: 4),
                Text(
                  'Wants to run $toolName',
                  style: TextStyle(fontSize: 13, color: c.warning, fontWeight: FontWeight.w500),
                ),
              ],
            ),
            // Command preview
            if (cmd != null) ...[
              const SizedBox(height: 6),
              Container(
                width: double.infinity,
                padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
                decoration: BoxDecoration(
                  color: c.ground,
                  borderRadius: BorderRadius.circular(8),
                ),
                child: Text(
                  cmd,
                  style: TextStyle(
                    fontFamily: 'Menlo',
                    fontFamilyFallback: const ['SF Mono', 'RobotoMono', 'monospace'],
                    fontSize: 12,
                    color: c.text,
                  ),
                ),
              ),
            ],
            const SizedBox(height: 10),
            // Deny / Allow
            Row(
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                OutlinedButton(
                  onPressed: () => onAnswer(false),
                  style: OutlinedButton.styleFrom(
                    foregroundColor: c.error,
                    side: BorderSide(color: c.error.withValues(alpha: .5)),
                    padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 6),
                    minimumSize: Size.zero,
                    tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                    shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(20)),
                  ),
                  child: const Text('Deny'),
                ),
                const SizedBox(width: 8),
                FilledButton(
                  onPressed: () => onAnswer(true),
                  style: FilledButton.styleFrom(
                    backgroundColor: c.warning,
                    foregroundColor: c.ground,
                    padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 6),
                    minimumSize: Size.zero,
                    tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                    shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(20)),
                  ),
                  child: const Text('Allow'),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

// ── Messenger-style bot row ───────────────────────────────────────────────────

class _BotRow extends StatelessWidget {
  const _BotRow({required this.entry, required this.showServer, required this.onTap});

  final ({BotListEntry bot, MacDevice device, String hostName, bool online}) entry;
  final bool showServer;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final bot = entry.bot;
    final lastTs = bot.last?.ts;
    final preview = _previewText(bot);
    final isWorking = bot.state == 'working';
    final needsYou = bot.state == 'needs_you';

    return InkWell(
      onTap: entry.online ? onTap : null,
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 10),
        child: Row(
          children: [
            // Avatar with optional red dot
            Stack(
              clipBehavior: Clip.none,
              children: [
                _Avatar(id: bot.id, name: bot.name, size: 24),
                if (needsYou)
                  Positioned(
                    right: -1,
                    top: -1,
                    child: Container(
                      width: 13,
                      height: 13,
                      decoration: BoxDecoration(
                        color: c.error,
                        shape: BoxShape.circle,
                        border: Border.all(color: c.ground, width: 2),
                      ),
                    ),
                  ),
              ],
            ),
            const SizedBox(width: 12),
            // Name + preview
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(
                    children: [
                      Expanded(
                        child: Text(
                          bot.name,
                          style: TextStyle(
                            fontWeight: FontWeight.w600,
                            fontSize: 15,
                            color: entry.online ? c.text : c.faint,
                          ),
                          overflow: TextOverflow.ellipsis,
                        ),
                      ),
                      if (lastTs != null)
                        Text(
                          _timeAgo(lastTs),
                          style: TextStyle(fontSize: 12, color: c.faint),
                        ),
                    ],
                  ),
                  const SizedBox(height: 2),
                  Row(
                    children: [
                      Expanded(
                        child: Text(
                          preview,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: TextStyle(
                            fontSize: 13,
                            color: isWorking ? c.accent : c.muted,
                            fontStyle: isWorking ? FontStyle.italic : FontStyle.normal,
                          ),
                        ),
                      ),
                      if (showServer && entry.hostName.isNotEmpty) ...[
                        const SizedBox(width: 6),
                        Text(
                          entry.hostName,
                          style: TextStyle(fontSize: 11, color: c.faint),
                        ),
                      ],
                    ],
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

// ── Shared avatar ─────────────────────────────────────────────────────────────

class _Avatar extends StatelessWidget {
  const _Avatar({required this.id, required this.name, required this.size});
  final String id;
  final String name;
  final double size; // radius

  @override
  Widget build(BuildContext context) {
    final color = _botColor(id);
    final initial = name.isNotEmpty ? name[0].toUpperCase() : '?';
    return CircleAvatar(
      radius: size,
      backgroundColor: color.withValues(alpha: .22),
      child: Text(
        initial,
        style: TextStyle(
          color: color,
          fontSize: size * 0.78,
          fontWeight: FontWeight.w700,
        ),
      ),
    );
  }
}
