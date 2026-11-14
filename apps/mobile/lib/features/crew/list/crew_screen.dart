/// The Crew tab: the deck of every crewmate across the paired computers and their crew
/// servers, the "Captain's call" approvals, and the logbook. Also listens for crew-server
/// invites pushed to the phone.
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/crew_models.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/providers/push_events_provider.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/core/ui/floating_tab_bar.dart';
import 'package:atlas_mobile/features/crew/crew_provider.dart';
import 'package:atlas_mobile/features/crew/invite_sheet.dart';
import 'package:atlas_mobile/features/crew/list/crew_deck.dart';
import 'package:atlas_mobile/features/crew/list/crewmate_row.dart';
import 'package:atlas_mobile/features/crew/list/needs_you_card.dart';
import 'package:atlas_mobile/features/crew/list/pause_card.dart';
import 'package:atlas_mobile/features/crew/new_crewmate_sheet.dart';

class CrewScreen extends ConsumerStatefulWidget {
  const CrewScreen({super.key});
  @override
  ConsumerState<CrewScreen> createState() => _CrewScreenState();
}

class _CrewScreenState extends ConsumerState<CrewScreen> {
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

    // Collect and sort all crewmates across devices.
    final allEntries = <CrewEntry>[];
    final crewServerDevices = <MacDevice>[];
    // Each crew server with something to pause, and whether it is paused.
    final pausable = <({MacDevice device, String name, bool paused})>[];
    // The computer's name only matters when crewmates live on more than one.

    for (final device in devices) {
      final data = ref.watch(crewProvider(device)).value;
      final online =
          ref.watch(connectionNotifierProvider(device)).value != null;
      if (data != null) {
        if (data.isCrewServer) crewServerDevices.add(device);
        if (data.isCrewServer && data.crewmates.isNotEmpty) {
          pausable.add((
            device: device,
            name: data.hostName,
            paused: data.paused,
          ));
        }
        for (final crewmate in data.crewmates) {
          allEntries.add((
            crewmate: crewmate,
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
      final ra = a.crewmate.state == 'needs_you'
          ? 0
          : a.crewmate.state == 'working'
          ? 1
          : 2;
      final rb = b.crewmate.state == 'needs_you'
          ? 0
          : b.crewmate.state == 'working'
          ? 1
          : 2;
      if (ra != rb) return ra.compareTo(rb);
      final ta = a.crewmate.last?.ts ?? 0;
      final tb = b.crewmate.last?.ts ?? 0;
      return tb.compareTo(ta);
    });

    final needsYou = allEntries
        .where((e) => e.crewmate.state == 'needs_you')
        .toList();

    return Scaffold(
      body: Stack(
        children: [
          // The crew's lantern: a glow behind the crew that starts above the status
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
                      message: 'Set up a crew server first, then recruit crewmates from Atlas Desktop.',
                      action: crewServerDevices.isEmpty
                          ? null
                          : FilledButton(
                              onPressed: () => showNewCrewmateSheet(
                                context,
                                crewServerDevices,
                              ),
                              child: const Text('Recruit'),
                            ),
                    ),
                  )
                : CustomScrollView(
                    slivers: [
                      for (final p in pausable)
                        SliverToBoxAdapter(
                          child: PauseCard(
                            serverName: p.name,
                            paused: p.paused,
                            showServer: pausable.length > 1,
                            onSet: (v) => setCrewPaused(ref, p.device, v),
                          ),
                        ),
                      SliverToBoxAdapter(
                        child: CrewDeck(
                          entries: allEntries,
                          onOpen: (e) => _openCrewmate(context, e),
                          onRecruit: crewServerDevices.isEmpty
                              ? null
                              : () => showNewCrewmateSheet(
                                  context,
                                  crewServerDevices,
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
                              NeedsYouCard(
                                entry: e,
                                onAnswer: (allow) => answerApproval(
                                  ref,
                                  e.device,
                                  e.crewmate.pendingApproval!,
                                  allow,
                                ),
                                onTap: () => _openCrewmate(context, e),
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
                              child: CrewmateRow(
                                entry: allEntries[i],
                                showServer: multipleServers,
                                last: i == allEntries.length - 1,
                                onTap: () =>
                                    _openCrewmate(context, allEntries[i]),
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

  void _openCrewmate(BuildContext context, CrewEntry e) {
    context.push(
      '/crew/${e.device.nodeId}/${e.crewmate.id}',
      extra: {'device': e.device},
    );
  }
}
