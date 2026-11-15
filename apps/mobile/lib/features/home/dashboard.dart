import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/protocol/latest_only.dart';
import 'package:atlas_mobile/core/protocol/pty_open.dart';
import 'package:atlas_mobile/core/ui/async_view.dart';
import 'package:atlas_mobile/core/ui/computer_state.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/core/ui/floating_tab_bar.dart';
import 'package:atlas_mobile/features/agents/agents_provider.dart';
import 'package:atlas_mobile/features/home/home_cards.dart';
import 'package:atlas_mobile/features/home/home_header.dart';
import 'package:atlas_mobile/features/home/project_tile.dart';
import 'package:atlas_mobile/features/inbox/inbox_providers.dart';
import 'package:atlas_mobile/features/projects/projects_provider.dart';
import 'package:atlas_mobile/features/terminal/new_terminal.dart';

class HomeDashboard extends ConsumerStatefulWidget {
  const HomeDashboard({
    super.key,
    required this.device,
    required this.others,
    required this.onSwitch,
  });

  final MacDevice device;
  final List<MacDevice> others;
  final void Function(MacDevice) onSwitch;

  @override
  ConsumerState<HomeDashboard> createState() => HomeDashboardState();
}

class HomeDashboardState extends ConsumerState<HomeDashboard> {
  Timer? _timer;
  final _poll = LatestOnly();

  @override
  void initState() {
    super.initState();
    // Services have no push: poll the project list. Agent states are pushed live.
    _timer = Timer.periodic(const Duration(seconds: 5), (_) => _tick());
  }

  /// One poll: only in the foreground, and never behind an unanswered one. A computer that is not
  /// connected sends nothing (`requireMux` throws before any command) and the screen keeps its list.
  Future<void> _tick() async {
    if (!appInForeground()) return;
    final ticket = _poll.tryBegin();
    if (ticket == null) return;
    final provider = projectListProvider(widget.device);
    try {
      ref.invalidate(provider);
      await ref.read(provider.future);
    } catch (_) {
      // The screen shows the failure (or keeps the last list); a poll has nobody to tell.
    } finally {
      ticket.done();
    }
  }

  @override
  void dispose() {
    _poll.close();
    _timer?.cancel();
    super.dispose();
  }

  void _openProject(ProjectInfo p) =>
      context.push('/project', extra: {'device': widget.device, 'project': p});

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final conn = ref.watch(connectionNotifierProvider(widget.device));
    final connected = conn.value != null;
    // Always watch so the last loaded value survives reconnects.
    final projects = ref.watch(projectListProvider(widget.device));
    // Live agent states override the list's snapshot as soon as they are pushed.
    final live = connected
        ? ref.watch(agentStatusMapNotifierProvider(widget.device)).value ??
              const {}
        : const <String, AgentStatus>{};

    final list = projects.value ?? const <ProjectInfo>[];
    final agentsOf = <String, List<(String, AgentStatus)>>{};
    for (final p in list) {
      for (final (pane, s0) in p.agents) {
        (agentsOf[p.id] ??= []).add((pane, live[pane] ?? s0));
      }
    }
    final attention = attentionFor(
      widget.device,
      projects: list,
      live: live,
      crewmates: const [],
    );
    // Projects that need you, then running, then stopped.
    final flagged = {
      for (final a in attention)
        if (a
            case AgentAttention(:final project) ||
                ServiceAttention(:final project))
          project.id,
    };
    final sorted = [...list]
      ..sort((a, b) {
        int rank(ProjectInfo p) => flagged.contains(p.id)
            ? 0
            : p.running
            ? 1
            : 2;
        return rank(a).compareTo(rank(b));
      });
    final missing = connected
        ? ref.watch(missingIntegrationsProvider(widget.device)).value ??
              const <AgentIntegration>[]
        : const <AgentIntegration>[];
    final working = agentsOf.values
        .expand((v) => v)
        .where((a) => a.$2.state == 'working')
        .length;

    return Stack(
      children: [
        // The app icon's glow, faint, behind the header: Atlas, not a generic list.
        Positioned(
          top: -160,
          left: -80,
          right: -80,
          height: 420,
          child: IgnorePointer(
            child: DecoratedBox(
              decoration: BoxDecoration(
                gradient: RadialGradient(
                  colors: [
                    const Color(0xFF3D5BFF).withValues(alpha: .22),
                    const Color(0xFF3D5BFF).withValues(alpha: 0),
                  ],
                ),
              ),
            ),
          ),
        ),
        // The list starts below the status bar: what scrolls away is cut there, never drawn under the clock.
        SafeArea(
          bottom: false,
          child: RefreshIndicator(
            color: c.accent,
            onRefresh: () async {
              if (!connected) {
                ref.invalidate(connectionNotifierProvider(widget.device));
              } else {
                ref.invalidate(projectListProvider(widget.device));
                ref.invalidate(missingIntegrationsProvider(widget.device));
              }
            },
            child: CustomScrollView(
              physics: const AlwaysScrollableScrollPhysics(),
              slivers: [
                SliverToBoxAdapter(
                  child: HomeHeader(
                    device: widget.device,
                    conn: conn,
                    others: widget.others,
                    onSwitch: widget.onSwitch,
                    status: !connected || projects.value == null
                        ? null
                        : attention.isNotEmpty
                        ? (
                            '${attention.length} ${attention.length == 1 ? 'needs' : 'need'} you',
                            c.warning,
                          )
                        : working > 0
                        ? (
                            '$working ${working == 1 ? 'agent' : 'agents'} working',
                            c.accent,
                          )
                        : ('All clear', c.success),
                  ),
                ),
                // ── Content slivers ──────────────────────────────────────────
                if (!connected && !projects.hasValue)
                  // EmptyState scrolls itself (a LayoutBuilder: no intrinsic sizes), so the
                  // sliver hands it all the room that is left.
                  SliverFillRemaining(
                    child: Padding(
                      padding: const EdgeInsets.only(top: Space.xxl),
                      child: ReachingView(
                        device: widget.device,
                        connecting: conn.isLoading,
                      ),
                    ),
                  )
                else ...[
                  // The computer is out of reach but its last list is still here.
                  if (!connected)
                    SliverToBoxAdapter(
                      child: OfflineStrip(
                        device: widget.device,
                        suffix: ' · projects shown',
                      ),
                    ),
                  // Attention cards + integrations + section header
                  SliverPadding(
                    padding: const EdgeInsets.fromLTRB(
                      Space.lg,
                      Space.sm,
                      Space.lg,
                      0,
                    ),
                    sliver: SliverList.list(
                      children: [
                        for (final a in missing) HomeIntegrationOff(agent: a),
                        if (missing.isNotEmpty)
                          const SizedBox(height: Space.md),
                        SectionHeader(
                          'Projects',
                          count: list.length,
                          trailing: canOpenTerminal(widget.device)
                              ? TextButton.icon(
                                  key: const Key('new-terminal-computer'),
                                  onPressed: () => newTerminalOnComputer(
                                    context,
                                    ref,
                                    widget.device,
                                  ),
                                  icon: const Icon(
                                    Icons.terminal_rounded,
                                    size: 18,
                                  ),
                                  label: const Text('New terminal'),
                                )
                              : null,
                        ),
                      ],
                    ),
                  ),
                  // The project list, kept through a reconnect or a failed refresh; "No projects
                  // yet" only when the computer answered "none". Nothing loaded yet: the spinner,
                  // or the error with "Try again" (AsyncView).
                  if (!projects.hasValue)
                    SliverFillRemaining(
                      child: AsyncView<List<ProjectInfo>>(
                        value: projects,
                        errorTitle: "Couldn't read the projects",
                        onRetry: () =>
                            ref.invalidate(projectListProvider(widget.device)),
                        data: (_) => const SizedBox.shrink(),
                      ),
                    )
                  else if (list.isEmpty)
                    const SliverFillRemaining(
                      child: Padding(
                        padding: EdgeInsets.only(bottom: kFloatingBarScrollPad),
                        child: HomeNoProjects(),
                      ),
                    )
                  else
                    SliverPadding(
                      padding: const EdgeInsets.fromLTRB(
                        0,
                        Space.xs,
                        0,
                        Space.xxl + kFloatingBarScrollPad,
                      ),
                      sliver: SliverList.list(
                        children: [
                          for (var i = 0; i < sorted.length; i++)
                            ProjectTile(
                              project: sorted[i],
                              agents: agentsOf[sorted[i].id] ?? const [],
                              flagged: flagged.contains(sorted[i].id),
                              onOpen: () => _openProject(sorted[i]),
                              last: i == sorted.length - 1,
                            ),
                        ],
                      ),
                    ),
                ],
              ],
            ),
          ),
        ),
      ],
    );
  }
}
