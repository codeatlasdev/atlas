/// The Crew tab: the deck of every crewmate across the paired computers and their crew
/// servers, the "Captain's call" approvals, and the logbook. Also listens for crew-server
/// invites pushed to the phone.
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import 'package:atlas_mobile/core/ui/route_page.dart';
import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/connection/require_mux.dart';
import 'package:atlas_mobile/core/protocol/crew_models.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/providers/push_events_provider.dart';
import 'package:atlas_mobile/core/ui/async_view.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/attention_card.dart';
import 'package:atlas_mobile/core/ui/computer_state.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/core/ui/floating_tab_bar.dart';
import 'package:atlas_mobile/core/ui/protocol_banner.dart';
import 'package:atlas_mobile/features/crew/crew_provider.dart';
import 'package:atlas_mobile/features/crew/invite_sheet.dart';
import 'package:atlas_mobile/features/crew/list/crew_skeleton.dart';
import 'package:atlas_mobile/features/crew/list/crew_deck.dart';
import 'package:atlas_mobile/features/crew/list/crewmate_row.dart';
import 'package:atlas_mobile/features/crew/list/needs_you_card.dart';
import 'package:atlas_mobile/features/crew/list/pause_card.dart';
import 'package:atlas_mobile/features/crew/new_crewmate_sheet.dart';
import 'package:atlas_mobile/features/crew/ops/crew_audit_screen.dart';
import 'package:atlas_mobile/features/crew/ops/crew_settings_screen.dart';

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
    final devicesAsync = ref.watch(crewDevicesProvider);
    // `.value` is null until the computers are listed (a spinner below) or when listing failed (an error below):
    // never an empty list that reads as "no computers, no crew".
    final devices = devicesAsync.value ?? const <MacDevice>[];

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
    // Invites already waiting when the phone connected: no push announces them any more.
    ref.listen(pendingInvitesProvider, (_, next) {
      next.whenData((pending) {
        for (final p in pending) {
          _inviteDeviceMap[p.invite.url] = p.device;
          ref.read(computerInvitesProvider.notifier).add(p.invite);
        }
      });
    });
    ref.listen<List<ComputerInvite>>(computerInvitesProvider, (_, invites) {
      _onNewInvites(invites);
    });

    // Collect and sort all crewmates across devices.
    final allEntries = <CrewEntry>[];
    final crewServerDevices = <MacDevice>[];
    // Each crew server with something to pause, and whether it is paused.
    final pausable = <({MacDevice device, String name, bool paused})>[];

    // Nothing to list yet: the computers, or any computer's crew, are still loading. Not "No crew yet".
    var loading = devicesAsync.isLoading && !devicesAsync.hasValue;
    // Computers whose crew could not be read and that have nothing to show from before: why, per computer.
    final failed = <({MacDevice device, Object error})>[];
    // Computers still on their first answer: neither their crew nor an error to show yet.
    final reaching = <MacDevice>[];
    for (final device in devices) {
      final crew = ref.watch(crewProvider(device));
      if (crew.isLoading && !crew.hasValue) {
        loading = true;
        reaching.add(device);
      }
      if (crew.hasError && !crew.hasValue) {
        failed.add((device: device, error: crew.error!));
      }
      // `value` of an `AsyncValue` that is reloading or in error keeps the last answer: the crew stays on
      // screen while the link comes back.
      final data = crew.value;
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

    final needsYou = allEntries.where(needsCaptain).toList();
    final mismatch = [
      for (final d in devices) ?ref.watch(protocolMismatchProvider(d)),
    ].firstOrNull;

    return Scaffold(
      body: Stack(
        children: [
          SafeArea(
            bottom: false,
            child: allEntries.isEmpty && loading && failed.isEmpty
                ? const CrewSkeleton()
                : allEntries.isEmpty && devicesAsync.hasError
                ? _CrewError(
                    title: "Couldn't list your computers",
                    error: devicesAsync.error!,
                    onRetry: () => ref.invalidate(crewDevicesProvider),
                  )
                : allEntries.isEmpty && devices.length == 1 && failed.isNotEmpty
                ? _CrewError(
                    title: "Couldn't read the crew",
                    error: failed.single.error,
                    onRetry: () => ref.invalidate(crewProvider(devices.single)),
                  )
                : allEntries.isEmpty && failed.isNotEmpty
                // Several computers, none loaded: each one's own state (its error and its own "Try again", or
                // still reaching), never only the first computer's error and one "Try again" for all.
                ? ListView(
                    padding: const EdgeInsets.only(
                      top: Space.lg,
                      bottom: Space.xl + kFloatingBarScrollPad,
                    ),
                    children: [
                      ProtocolBanner(devices: devices),
                      for (final d in reaching) _ComputerReaching(device: d),
                      for (final f in failed)
                        _ComputerFailed(
                          device: f.device,
                          error: f.error,
                          onRetry: () => ref.invalidate(crewProvider(f.device)),
                        ),
                    ],
                  )
                : allEntries.isEmpty
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
                      // A computer too old to list its crew shows none: say why, not "No crew yet".
                      title: mismatch?.title ?? 'No crew yet',
                      message: mismatch?.message ?? 'Set up a crew server first, then recruit crewmates from Atlas Desktop.',
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
                      SliverToBoxAdapter(
                        child: ProtocolBanner(devices: devices),
                      ),
                      for (final p in pausable)
                        SliverToBoxAdapter(
                          child: Column(
                            children: [
                              PauseCard(
                                serverName: p.name,
                                paused: p.paused,
                                showServer: pausable.length > 1,
                                onSet: (v) => setCrewPaused(ref, p.device, v),
                              ),
                              _CrewOpsMenu(
                                device: p.device,
                                serverName: p.name,
                                showServer: pausable.length > 1,
                              ),
                            ],
                          ),
                        ),
                      // A computer that has not answered (or failed) says so on its own, above the crew of
                      // the computers that did; its "Try again" asks only that computer.
                      for (final d in reaching)
                        SliverToBoxAdapter(child: _ComputerReaching(device: d)),
                      for (final f in failed)
                        SliverToBoxAdapter(
                          child: _ComputerFailed(
                            device: f.device,
                            error: f.error,
                            onRetry: () =>
                                ref.invalidate(crewProvider(f.device)),
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
                                onAnswer: (allow) =>
                                    answerApproval(
                                      ref,
                                      e.device,
                                      e.crewmate.pendingApproval!,
                                      allow,
                                    ).catchError((Object err) {
                                      if (context.mounted) {
                                        ScaffoldMessenger.of(context)
                                            .showSnackBar(
                                              SnackBar(
                                                content: Text(errorText(err)),
                                              ),
                                            );
                                      }
                                    }),
                                onAnswerTask: (value) =>
                                    answerWaitingAction(
                                      ref,
                                      e.device,
                                      e.crewmate.id,
                                      e.crewmate.waitingTask!,
                                      value,
                                    ).catchError((Object err) {
                                      if (context.mounted) {
                                        ScaffoldMessenger.of(context)
                                            .showSnackBar(
                                              SnackBar(content: Text('$err')),
                                            );
                                      }
                                    }),
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

/// A computer whose crew is still being asked for: its name and a spinner, beside the crew of the others.
class _ComputerReaching extends StatelessWidget {
  const _ComputerReaching({required this.device});
  final MacDevice device;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Padding(
      padding: const EdgeInsets.fromLTRB(Space.lg, 0, Space.lg, Space.md),
      child: Row(
        children: [
          const SizedBox.square(
            dimension: 16,
            child: CircularProgressIndicator(strokeWidth: 2),
          ),
          const SizedBox(width: Space.sm),
          Text(
            'Reaching ${device.name}…',
            style: Theme.of(context).textTheme.bodyMedium!
                .copyWith(color: c.muted),
          ),
        ],
      ),
    );
  }
}

/// One computer whose crew could not be read, with its own "Try again". A computer that is off says so with the
/// shared sentence ([DeviceOffline]) and retries its connection; one on another protocol shows its update
/// message and no retry (it would fail the same way); anything else shows the server's own error, named.
class _ComputerFailed extends ConsumerWidget {
  const _ComputerFailed({
    required this.device,
    required this.error,
    required this.onRetry,
  });
  final MacDevice device;
  final Object error;
  final VoidCallback onRetry;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final (title, body, retry) = switch (error) {
      DeviceOffline() => (
        "Can't reach ${device.name}",
        unreachableMessage,
        () => ref.invalidate(connectionNotifierProvider(device)),
      ),
      DeviceProtocolMismatch(:final mismatch) => (
        mismatch.title,
        mismatch.message,
        null,
      ),
      _ => ("Couldn't read ${device.name}'s crew", errorText(error), onRetry),
    };
    return AttentionCard(
      margin: const EdgeInsets.fromLTRB(Space.lg, 0, Space.lg, Space.md),
      icon: Icons.error_outline_rounded,
      title: title,
      body: body,
      actions: [
        if (retry != null)
          OutlinedButton(onPressed: retry, child: const Text('Try again')),
      ],
    );
  }
}

/// The Crew tab when no crew could be read: the shared offline / update / error state with "Try again"
/// (`AsyncView`'s words), under the tab's floating bar.
class _CrewError extends StatelessWidget {
  const _CrewError({
    required this.title,
    required this.error,
    required this.onRetry,
  });
  final String title;
  final Object error;
  final VoidCallback onRetry;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.only(bottom: kFloatingBarScrollPad),
    child: AsyncView<void>(
      value: AsyncError<void>(error, StackTrace.empty),
      data: (_) => const SizedBox.shrink(),
      errorTitle: title,
      onRetry: onRetry,
    ),
  );
}

/// Crew settings and the crew-wide audit of one crew server, under its pause switch.
class _CrewOpsMenu extends StatelessWidget {
  const _CrewOpsMenu({
    required this.device,
    required this.serverName,
    required this.showServer,
  });
  final MacDevice device;
  final String serverName;
  final bool showServer;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return Padding(
      padding: const EdgeInsets.fromLTRB(Space.lg, 0, Space.lg, 0),
      child: Align(
        alignment: Alignment.centerRight,
        child: PopupMenuButton<String>(
          key: const Key('crew-ops-menu'),
          tooltip: showServer ? 'Crew options · $serverName' : 'Crew options',
          icon: Icon(Icons.tune_rounded, size: 20, color: c.muted),
          onSelected: (v) => pushScreen<void>(
            context,
            v == 'settings'
                ? CrewSettingsScreen(device: device)
                : CrewAuditScreen(device: device, title: serverName),
          ),
          itemBuilder: (_) => const [
            PopupMenuItem(value: 'settings', child: Text('Crew settings')),
            PopupMenuItem(value: 'audit', child: Text('Crew audit')),
          ],
        ),
      ),
    );
  }
}
