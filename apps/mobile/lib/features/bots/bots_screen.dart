import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/providers/push_events_provider.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/features/bots/bots_provider.dart';
import 'package:atlas_mobile/features/bots/new_bot_sheet.dart';
import 'package:atlas_mobile/features/bots/invite_sheet.dart';
import 'package:atlas_mobile/core/ui/floating_tab_bar.dart';
import 'package:atlas_mobile/features/bots/crew_text.dart';
import 'package:atlas_mobile/features/bots/crewmate.dart';

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

  Future<void> _approve(
    MacDevice device,
    BotPendingApproval pa,
    bool allow,
  ) async {
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
    final devices = ref.watch(crewDevicesProvider).value ?? [];

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
    final allEntries =
        <
          ({BotListEntry bot, MacDevice device, String hostName, bool online})
        >[];
    final botServerDevices = <MacDevice>[];
    // The computer's name only matters when crewmates live on more than one.

    for (final device in devices) {
      final data = ref.watch(botsProvider(device)).value;
      final online =
          ref.watch(connectionNotifierProvider(device)).value != null;
      if (data != null) {
        if (data.isBotServer) botServerDevices.add(device);
        for (final bot in data.bots) {
          allEntries.add((
            bot: bot,
            device: device,
            hostName: data.hostName,
            online: online,
          ));
        }
      }
    }

    final multipleServers =
        allEntries.map((e) => e.device.nodeId).toSet().length > 1;

    // Sort: needs_you first, then by last.ts desc.
    allEntries.sort((a, b) {
      final ra = a.bot.state == 'needs_you'
          ? 0
          : a.bot.state == 'working'
          ? 1
          : 2;
      final rb = b.bot.state == 'needs_you'
          ? 0
          : b.bot.state == 'working'
          ? 1
          : 2;
      if (ra != rb) return ra.compareTo(rb);
      final ta = a.bot.last?.ts ?? 0;
      final tb = b.bot.last?.ts ?? 0;
      return tb.compareTo(ta);
    });

    final needsYou = allEntries
        .where((e) => e.bot.state == 'needs_you')
        .toList();

    return Scaffold(
      body: Stack(
        children: [
          // The ship's lantern: a glow behind the crew that starts above the status
          // bar, so it never meets the screen's top edge as a hard line.
          Positioned(
            top: -200,
            left: -80,
            right: -80,
            height: 520,
            child: IgnorePointer(
              child: DecoratedBox(
                decoration: BoxDecoration(
                  gradient: RadialGradient(
                    colors: [
                      c.accent.withValues(alpha: .18),
                      c.accent.withValues(alpha: 0),
                    ],
                  ),
                ),
              ),
            ),
          ),
          SafeArea(
            bottom: false,
            child: allEntries.isEmpty
                ? Padding(
                    padding: const EdgeInsets.only(
                      bottom: kFloatingBarScrollPad,
                    ),
                    child: EmptyState(
                      icon: ShipFlagIcon(
                        color: c.faint,
                        accentColor: c.accent,
                        size: 64,
                      ),
                      title: 'No crew yet',
                      message: 'Launch your ship first, then recruit crewmates from Atlas Desktop.',
                      action: botServerDevices.isEmpty
                          ? null
                          : FilledButton(
                              onPressed: () => showModalBottomSheet<void>(
                                context: context,
                                isScrollControlled: true,
                                showDragHandle: true,
                                builder: (_) =>
                                    NewBotSheet(devices: botServerDevices),
                              ),
                              child: const Text('Recruit'),
                            ),
                    ),
                  )
                : CustomScrollView(
                    slivers: [
                      SliverToBoxAdapter(
                        child: _Deck(
                          entries: allEntries,
                          onOpen: (e) => _openBot(context, e),
                          onRecruit: botServerDevices.isEmpty
                              ? null
                              : () => showModalBottomSheet<void>(
                                  context: context,
                                  isScrollControlled: true,
                                  showDragHandle: true,
                                  builder: (_) =>
                                      NewBotSheet(devices: botServerDevices),
                                ),
                        ),
                      ),
                      if (needsYou.isNotEmpty) ...[
                        const SliverToBoxAdapter(
                          child: Padding(
                            padding: EdgeInsets.fromLTRB(
                              Space.lg,
                              Space.md,
                              Space.lg,
                              0,
                            ),
                            child: SectionHeader("Captain's call"),
                          ),
                        ),
                        SliverList.list(
                          children: [
                            for (final e in needsYou)
                              _NeedsYouCard(
                                entry: e,
                                onAnswer: (allow) => _approve(
                                  e.device,
                                  e.bot.pendingApproval!,
                                  allow,
                                ),
                                onTap: () => _openBot(context, e),
                              ),
                          ],
                        ),
                      ],
                      const SliverToBoxAdapter(
                        child: Padding(
                          padding: EdgeInsets.fromLTRB(
                            Space.lg,
                            Space.md,
                            Space.lg,
                            0,
                          ),
                          child: SectionHeader('Logbook'),
                        ),
                      ),
                      SliverList.list(
                        children: [
                          for (var i = 0; i < allEntries.length; i++)
                            EnterFade(
                              index: i,
                              child: _BotRow(
                                entry: allEntries[i],
                                showServer: multipleServers,
                                last: i == allEntries.length - 1,
                                onTap: () => _openBot(context, allEntries[i]),
                              ),
                            ),
                        ],
                      ),
                      const SliverToBoxAdapter(
                        child: SizedBox(
                          height: Space.xl + kFloatingBarScrollPad,
                        ),
                      ),
                    ],
                  ),
          ),
        ],
      ),
    );
  }

  void _openBot(
    BuildContext context,
    ({BotListEntry bot, MacDevice device, String hostName, bool online}) e,
  ) {
    context.push(
      '/bots/${e.device.nodeId}/${e.bot.id}',
      extra: {'device': e.device},
    );
  }
}

// ── Needs-you card ────────────────────────────────────────────────────────────

class _NeedsYouCard extends StatelessWidget {
  const _NeedsYouCard({
    required this.entry,
    required this.onAnswer,
    required this.onTap,
  });

  final ({BotListEntry bot, MacDevice device, String hostName, bool online})
  entry;
  final void Function(bool allow) onAnswer;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final pa = entry.bot.pendingApproval;
    final toolName = pa?.toolName ?? 'a tool';
    final cmd =
        pa?.toolInput is Map && (pa!.toolInput as Map)['command'] is String
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
                Crewmate(
                  seed: entry.bot.id,
                  look: entry.bot.look,
                  state: CrewState.needsYou,
                  size: 32,
                ),
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
                  style: TextStyle(
                    fontSize: 13,
                    color: c.warning,
                    fontWeight: FontWeight.w500,
                  ),
                ),
              ],
            ),
            // Command preview
            if (cmd != null) ...[
              const SizedBox(height: 6),
              Container(
                width: double.infinity,
                padding: const EdgeInsets.symmetric(
                  horizontal: 10,
                  vertical: 6,
                ),
                decoration: BoxDecoration(
                  color: c.ground,
                  borderRadius: BorderRadius.circular(8),
                ),
                child: Text(
                  cmd,
                  style: TextStyle(
                    fontFamily: 'Menlo',
                    fontFamilyFallback: const [
                      'SF Mono',
                      'RobotoMono',
                      'monospace',
                    ],
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
                    padding: const EdgeInsets.symmetric(
                      horizontal: 16,
                      vertical: 6,
                    ),
                    minimumSize: Size.zero,
                    tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                    shape: RoundedRectangleBorder(
                      borderRadius: BorderRadius.circular(20),
                    ),
                  ),
                  child: const Text('Deny'),
                ),
                const SizedBox(width: 8),
                FilledButton(
                  onPressed: () => onAnswer(true),
                  style: FilledButton.styleFrom(
                    backgroundColor: c.warning,
                    foregroundColor: c.ground,
                    padding: const EdgeInsets.symmetric(
                      horizontal: 16,
                      vertical: 6,
                    ),
                    minimumSize: Size.zero,
                    tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                    shape: RoundedRectangleBorder(
                      borderRadius: BorderRadius.circular(20),
                    ),
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

// ── The deck: every crewmate standing together ──────────────────────────────

typedef _Entry = ({
  BotListEntry bot,
  MacDevice device,
  String hostName,
  bool online,
});

class _Deck extends StatelessWidget {
  const _Deck({
    required this.entries,
    required this.onOpen,
    required this.onRecruit,
  });
  final List<_Entry> entries;
  final void Function(_Entry) onOpen;
  final VoidCallback? onRecruit;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final working = entries.where((e) => e.bot.state == 'working').length;
    final waiting = entries.where((e) => e.bot.state == 'needs_you').length;
    final (label, color) = waiting > 0
        ? ('$waiting ${waiting == 1 ? 'needs' : 'need'} you', c.warning)
        : working > 0
        ? ('$working working', c.accent)
        : ('All hands on deck', c.success);
    return Stack(
      children: [
        Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(
                Space.lg,
                Space.md,
                Space.lg,
                0,
              ),
              child: Row(
                children: [
                  ShipFlagIcon(color: c.muted, accentColor: c.accent, size: 22),
                  const SizedBox(width: Space.sm),
                  Text(
                    '${entries.length} aboard',
                    style: TextStyle(
                      color: c.muted,
                      fontWeight: FontWeight.w600,
                    ),
                  ),
                  const Spacer(),
                  _Pill(
                    label: label,
                    color: color,
                    pulse: working + waiting > 0,
                  ),
                ],
              ),
            ),
            SizedBox(
              height: 168,
              child: ListView(
                scrollDirection: Axis.horizontal,
                padding: const EdgeInsets.symmetric(horizontal: Space.md),
                children: [
                  for (var i = 0; i < entries.length; i++)
                    EnterFade(
                      index: i,
                      child: _DeckMate(
                        entry: entries[i],
                        onTap: () => onOpen(entries[i]),
                      ),
                    ),
                  if (onRecruit != null) _RecruitSlot(onTap: onRecruit!),
                ],
              ),
            ),
          ],
        ),
      ],
    );
  }
}

class _DeckMate extends StatelessWidget {
  const _DeckMate({required this.entry, required this.onTap});
  final _Entry entry;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final bot = entry.bot;
    final state = CrewState.of(bot.state, online: entry.online);
    return Semantics(
      button: true,
      label: '${bot.name}, ${_stateWord(state)}',
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: entry.online ? onTap : null,
        child: SizedBox(
          width: 96,
          child: Column(
            children: [
              const SizedBox(height: Space.md),
              Crewmate(seed: bot.id, look: bot.look, state: state, size: 92),
              const SizedBox(height: 4),
              Text(
                bot.name,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(
                  fontWeight: FontWeight.w700,
                  color: entry.online ? c.text : c.faint,
                ),
              ),
              const SizedBox(height: 2),
              _StateDot(state: state),
            ],
          ),
        ),
      ),
    );
  }
}

class _RecruitSlot extends StatelessWidget {
  const _RecruitSlot({required this.onTap});
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Semantics(
      button: true,
      label: 'Recruit a crewmate',
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: onTap,
        child: SizedBox(
          width: 96,
          child: Column(
            children: [
              const SizedBox(height: Space.md + 14),
              Container(
                width: 64,
                height: 64,
                decoration: BoxDecoration(
                  shape: BoxShape.circle,
                  color: c.accent.withValues(alpha: .08),
                  border: Border.all(
                    color: c.accent.withValues(alpha: .45),
                    width: 1.5,
                  ),
                ),
                child: Icon(Icons.add_rounded, color: c.accent, size: 30),
              ),
              const SizedBox(height: 18),
              Text(
                'Recruit',
                style: TextStyle(color: c.accent, fontWeight: FontWeight.w700),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

String _stateWord(CrewState s) => switch (s) {
  CrewState.working => 'working',
  CrewState.needsYou => 'needs you',
  CrewState.offline => 'offline',
  CrewState.idle => 'on deck',
};

class _StateDot extends StatelessWidget {
  const _StateDot({required this.state});
  final CrewState state;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final color = switch (state) {
      CrewState.working => c.accent,
      CrewState.needsYou => c.warning,
      CrewState.offline => c.faint,
      CrewState.idle => c.success,
    };
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        StatusDot(
          color: color,
          pulse: state == CrewState.working || state == CrewState.needsYou,
          size: 6,
        ),
        Flexible(
          child: Text(
            _stateWord(state),
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: TextStyle(
              fontSize: 11,
              color: color,
              fontWeight: FontWeight.w600,
            ),
          ),
        ),
      ],
    );
  }
}

class _Pill extends StatelessWidget {
  const _Pill({required this.label, required this.color, required this.pulse});
  final String label;
  final Color color;
  final bool pulse;

  @override
  Widget build(BuildContext context) => AnimatedSwitcher(
    duration: Motion.normal,
    child: Container(
      key: ValueKey(label),
      padding: const EdgeInsets.fromLTRB(2, 3, 10, 3),
      decoration: BoxDecoration(
        color: color.withValues(alpha: .12),
        borderRadius: BorderRadius.circular(999),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          StatusDot(color: color, pulse: pulse, size: 6),
          Text(
            label,
            style: TextStyle(
              color: color,
              fontSize: 12,
              fontWeight: FontWeight.w600,
            ),
          ),
        ],
      ),
    ),
  );
}

// ── Logbook: one card per crewmate, its last word ───────────────────────────

class _BotRow extends StatelessWidget {
  const _BotRow({
    required this.entry,
    required this.showServer,
    required this.onTap,
    this.last = false,
  });

  final _Entry entry;
  final bool showServer;
  final VoidCallback onTap;

  /// The last row draws no divider under it.
  final bool last;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final bot = entry.bot;
    final state = CrewState.of(bot.state, online: entry.online);
    final when = crewTimeAgo(bot.last?.ts ?? 0, DateTime.now());
    final (preview, previewColor) = switch (state) {
      CrewState.working => ('working…', c.accent),
      CrewState.needsYou when bot.pendingApproval != null => (
        'waiting for your approval',
        c.warning,
      ),
      CrewState.needsYou => ('needs you', c.warning),
      _ => (crewPreview(bot.last?.text ?? ''), c.muted),
    };
    // Telegram's chat row: avatar, name and time on one line, the last message under
    // it, a hairline divider that starts after the avatar.
    return Material(
      type: MaterialType.transparency,
      child: InkWell(
        onTap: entry.online ? onTap : null,
        child: Padding(
          padding: const EdgeInsets.only(left: Space.lg),
          child: Row(
            children: [
              Crewmate(
                seed: bot.id,
                look: bot.look,
                state: state,
                size: 54,
                animate:
                    state == CrewState.working || state == CrewState.needsYou,
              ),
              const SizedBox(width: 12),
              Expanded(
                child: Container(
                  padding: const EdgeInsets.fromLTRB(0, 12, Space.lg, 12),
                  decoration: BoxDecoration(
                    border: last
                        ? null
                        : Border(bottom: BorderSide(color: c.borderSubtle)),
                  ),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Row(
                        children: [
                          Expanded(
                            child: Text(
                              bot.name,
                              overflow: TextOverflow.ellipsis,
                              style: TextStyle(
                                fontWeight: FontWeight.w600,
                                fontSize: 16,
                                color: entry.online ? c.text : c.faint,
                              ),
                            ),
                          ),
                          if (when.isNotEmpty)
                            Text(
                              when,
                              style: TextStyle(fontSize: 13, color: c.faint),
                            ),
                        ],
                      ),
                      const SizedBox(height: 3),
                      Row(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Expanded(
                            child: Text(
                              preview,
                              maxLines: 2,
                              overflow: TextOverflow.ellipsis,
                              style: TextStyle(
                                fontSize: 14.5,
                                height: 1.3,
                                color: previewColor,
                              ),
                            ),
                          ),
                          if (state == CrewState.needsYou) ...[
                            const SizedBox(width: 8),
                            Container(
                              margin: const EdgeInsets.only(top: 2),
                              width: 20,
                              height: 20,
                              decoration: BoxDecoration(
                                color: c.warning,
                                shape: BoxShape.circle,
                              ),
                              alignment: Alignment.center,
                              child: Text(
                                '!',
                                style: TextStyle(
                                  color: c.ground,
                                  fontSize: 12,
                                  fontWeight: FontWeight.w800,
                                ),
                              ),
                            ),
                          ],
                        ],
                      ),
                      if (showServer && entry.hostName.isNotEmpty)
                        Padding(
                          padding: const EdgeInsets.only(top: 2),
                          child: Text(
                            entry.hostName,
                            style: TextStyle(fontSize: 12, color: c.faint),
                          ),
                        ),
                    ],
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
