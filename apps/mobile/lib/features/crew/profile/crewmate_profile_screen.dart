/// A crewmate's profile: what it does, how it works (approval, replies, memory), its automations,
/// its GitHub account, and removing it from the crew.
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/protocol/multiplexer.dart';
import 'package:atlas_mobile/core/connection/require_mux.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/protocol/crew_models.dart';
import 'package:atlas_mobile/core/protocol/crew_ops_models.dart';
import 'package:atlas_mobile/core/ui/async_view.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/crew/automations.dart';
import 'package:atlas_mobile/features/crew/automation_editor_sheet.dart';
import 'package:atlas_mobile/features/crew/crew_provider.dart';
import 'package:atlas_mobile/features/crew/chat_motion.dart';
import 'package:atlas_mobile/features/crew/ops/crew_audit_screen.dart';
import 'package:atlas_mobile/features/crew/ops/crew_ops_provider.dart';
import 'package:atlas_mobile/features/crew/ops/crewmate_usage_screen.dart';
import 'package:atlas_mobile/features/crew/ops/routines_section.dart';
import 'package:atlas_mobile/features/crew/profile/automations_section.dart';
import 'package:atlas_mobile/features/crew/profile/github_card.dart';
import 'package:atlas_mobile/features/crew/profile/profile_dialogs.dart';
import 'package:atlas_mobile/features/crew/profile/profile_header.dart';
import 'package:atlas_mobile/features/crew/profile/profile_rows.dart';
import 'package:atlas_mobile/features/crew/widgets/github_device_login.dart';
import 'package:atlas_mobile/features/crew/avatar/crewmate_look.dart';

class CrewmateProfileScreen extends ConsumerStatefulWidget {
  const CrewmateProfileScreen({
    super.key,
    required this.device,
    required this.crewmateId,
  });

  final MacDevice device;
  final String crewmateId;

  @override
  ConsumerState<CrewmateProfileScreen> createState() =>
      _CrewmateProfileScreenState();
}

class _CrewmateProfileScreenState extends ConsumerState<CrewmateProfileScreen> {
  CrewGithubStatus? _githubStatus;
  bool _githubLoading = false;
  late final _login = GithubDeviceLogin(onChanged: () => setState(() {}));

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) => _loadGithub());
  }

  Future<void> _loadGithub() async {
    final Multiplexer mux;
    try {
      mux = await requireMuxOf(ref, widget.device);
    } on DeviceUnavailable {
      return; // background load: the card just stays empty while the computer is off
    }
    final ghResp = await mux.call('crew_github_status', {});
    if (mounted && ghResp['ok'] == true) {
      final ghData = (ghResp['data'] as Map?)?.cast<String, dynamic>() ?? {};
      setState(() {
        _githubStatus = CrewGithubStatus.fromJson(ghData);
      });
    }
  }

  /// The live connection for something the user just pressed: when there is none, says why instead of doing nothing.
  Future<Multiplexer?> _mux() async {
    try {
      return await requireMuxOf(ref, widget.device);
    } on DeviceUnavailable catch (e) {
      if (mounted) _toast(e.message);
      return null;
    }
  }

  Future<void> _editRole(String current) async {
    final result = await askRole(context, current);
    if (result == null || !mounted) return;
    final mux = await _mux();
    if (mux == null) return;
    await mux.call('crewmate_update', {
      'id': widget.crewmateId,
      'role': result,
    });
    ref.invalidate(crewProvider(widget.device));
  }

  Future<void> _editMemory() async {
    final mux = await _mux();
    if (mux == null) return;

    final resp = await mux.call('crewmate_memory', {'id': widget.crewmateId});
    if (!mounted || resp['ok'] != true) return;
    final text = (resp['data'] as Map?)?['text'] as String? ?? '';
    final saved = await editMemory(context, text);
    if (saved == null || !mounted) return;
    await mux.call('crewmate_memory_set', {
      'id': widget.crewmateId,
      'text': saved,
    });
  }

  Future<void> _deleteCrewmate() async {
    if (!await confirmDeleteCrewmate(context) || !mounted) return;
    final mux = await _mux();
    if (mux == null) return;
    final resp = await mux.call('crewmate_delete', {'id': widget.crewmateId});
    if (!mounted) return;
    if (resp['ok'] == true) {
      Navigator.of(context).pop();
      ref.invalidate(crewProvider(widget.device));
    } else {
      _toast(resp['error'] as String? ?? 'Could not delete crewmate');
    }
  }

  // ── automations ───────────────────────────────────────────────────────────

  Future<void> _saveTriggers(List<Trigger> triggers) async {
    final mux = await _mux();
    if (mux == null) return;
    await mux.call('crewmate_triggers_set', {
      'id': widget.crewmateId,
      'triggers': triggers.map((t) => t.toJson()).toList(),
    });
    ref.invalidate(crewProvider(widget.device));
  }

  Future<void> _addAutomation(String crewmateName) async {
    final result = await showAutomationEditor(
      context,
      crewmateName: crewmateName,
    );
    if (result == null) return;
    final crewmate = ref
        .read(crewProvider(widget.device))
        .value
        ?.crewmates
        .where((b) => b.id == widget.crewmateId)
        .firstOrNull;
    await _saveTriggers([...?crewmate?.triggers, result]);
  }

  Future<void> _editAutomation(CrewmateEntry crewmate, Trigger trigger) async {
    final result = await showAutomationEditor(
      context,
      existing: trigger,
      crewmateName: crewmate.name,
    );
    if (result == null) return;
    await _saveTriggers([
      for (final t in crewmate.triggers)
        if (t.id == trigger.id) result else t,
    ]);
  }

  Future<void> _deleteAutomation(
    CrewmateEntry crewmate,
    Trigger trigger,
  ) async {
    if (!await confirmRemoveAutomation(
      context,
      describeTrigger(trigger.kind),
    )) {
      return;
    }
    await _saveTriggers(
      crewmate.triggers.where((t) => t.id != trigger.id).toList(),
    );
  }

  // ── routines ──────────────────────────────────────────────────────────────

  CrewmateEntry? get _crewmate => ref
      .read(crewProvider(widget.device))
      .value
      ?.crewmates
      .where((b) => b.id == widget.crewmateId)
      .firstOrNull;

  /// Saves the whole list; the error text hostd (or the connection) gave, or null when saved.
  Future<String?> _saveRoutines(List<Routine> routines) async {
    try {
      await saveRoutines(ref, widget.device, widget.crewmateId, routines);
      return null;
    } catch (e) {
      return e is StateError ? e.message : e.toString();
    }
  }

  Future<void> _editRoutine(Routine? existing) async {
    await showRoutineEditor(
      context,
      existing: existing,
      save: (r) =>
          _saveRoutines(upsertRoutine(_crewmate?.routines ?? const [], r)),
    );
  }

  Future<void> _toggleRoutine(Routine r, bool enabled) async {
    final err = await _saveRoutines(
      upsertRoutine(
        _crewmate?.routines ?? const [],
        r.copyWith(enabled: enabled),
      ),
    );
    if (err != null && mounted) _toast(err);
  }

  Future<void> _deleteRoutine(Routine r) async {
    if (!await confirmRemoveRoutine(context, r.prompt) || !mounted) return;
    final err = await _saveRoutines(
      removeRoutine(_crewmate?.routines ?? const [], r.id),
    );
    if (err != null && mounted) _toast(err);
  }

  void _open(Widget screen) =>
      Navigator.of(context)
          .push(MaterialPageRoute<void>(builder: (_) => screen));

  // ── github ────────────────────────────────────────────────────────────────

  Future<void> _startLogin() async {
    final mux = await _mux();
    if (mux == null) return;
    setState(() => _githubLoading = true);
    try {
      final error = await _login.start(mux, _pollLogin);
      if (error != null && mounted) _toast(error);
    } finally {
      if (mounted) setState(() => _githubLoading = false);
    }
  }

  Future<void> _pollLogin() async {
    final mux = liveMux(ref, widget.device);
    if (mux == null) return;
    final (result, data) = await _login.pollOnce(mux);
    if (!mounted) return;
    switch (result) {
      case GithubPoll.done:
        setState(() {
          _githubStatus = CrewGithubStatus(
            installed: true,
            login: data['login'] as String?,
          );
        });
      case GithubPoll.error:
        _toast(data['error']?.toString() ?? 'GitHub sign-in failed');
      case GithubPoll.pending:
    }
  }

  void _toast(String text) =>
      ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(text)));

  Future<void> _logout() async {
    final mux = await _mux();
    if (mux == null) return;
    await mux.call('crew_github_logout', {});
    if (!mounted) return;
    setState(() {
      _githubStatus = CrewGithubStatus(
        installed: _githubStatus?.installed ?? true,
        login: null,
      );
    });
  }

  @override
  void dispose() {
    _login.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final crew = ref.watch(crewProvider(widget.device));
    final crewData = crew.value;
    final crewmate = crewData?.crewmates
        .where((b) => b.id == widget.crewmateId)
        .firstOrNull;

    final hostName = crewData?.hostName ?? widget.device.name;
    final state = crewmate == null
        ? CrewState.idle
        : CrewState.of(crewmate.state);
    return Scaffold(
      body: crewmate == null
          ? AsyncView(
              value: crew,
              errorTitle: "Couldn't read the crewmate",
              onRetry: () => ref.invalidate(crewProvider(widget.device)),
              isEmpty: (_) => true,
              emptyTitle: 'Crewmate not found',
              emptyMessage: 'It may have been removed from its crew server.',
              data: (_) => const SizedBox.shrink(),
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
                              crewmate.look,
                              crewmate.id,
                            ).color.withValues(alpha: .26),
                            CrewLook.from(
                              crewmate.look,
                              crewmate.id,
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
                    ProfileHeader(
                      crewmate: crewmate,
                      hostName: hostName,
                      state: state,
                    ),
                    const SizedBox(height: Space.xl),

                    ProfileGroup(
                      title: 'What it does',
                      children: [
                        ProfileRow(
                          onTap: () => _editRole(crewmate.role),
                          child: Text(
                            crewmate.role.isNotEmpty
                                ? crewmate.role
                                : 'Tap to describe its job.',
                            style: Theme.of(context).textTheme.bodyLarge!
                                .copyWith(
                                  color: crewmate.role.isNotEmpty
                                      ? c.text
                                      : c.muted,
                                  height: 1.45,
                                ),
                          ),
                        ),
                      ],
                    ),

                    ProfileGroup(
                      title: 'How it works',
                      children: [
                        ProfileRow(
                          label: 'Approval',
                          child: ApprovalPicker(
                            value: crewmate.approval,
                            enabled: crewmate.approvalSupported,
                            onChanged: (val) async {
                              final mux = await _mux();
                              if (mux == null) return;
                              await mux.call('crewmate_update', {
                                'id': widget.crewmateId,
                                'approval': val,
                              });
                              ref.invalidate(crewProvider(widget.device));
                            },
                          ),
                        ),
                        ProfileRow(
                          label: 'Short replies',
                          child: ShortRepliesRow(
                            value: crewmate.shortReplies,
                            onChanged: (val) async {
                              final mux = await _mux();
                              if (mux == null) return;
                              await mux.call('crewmate_update', {
                                'id': widget.crewmateId,
                                'short_replies': val,
                              });
                              ref.invalidate(crewProvider(widget.device));
                            },
                          ),
                        ),
                        ProfileRow(
                          icon: Icons.auto_stories_outlined,
                          label: 'Memory',
                          trailingText: 'View / Edit',
                          onTap: _editMemory,
                        ),
                      ],
                    ),

                    AutomationsSection(
                      crewmate: crewmate,
                      now: DateTime.now(),
                      onAdd: () => _addAutomation(crewmate.name),
                      onEdit: (t) => _editAutomation(crewmate, t),
                      onDelete: (t) => _deleteAutomation(crewmate, t),
                      onToggle: (t, enabled) async {
                        await _saveTriggers([
                          for (final tr in crewmate.triggers)
                            if (tr.id == t.id)
                              tr.copyWith(enabled: enabled)
                            else
                              tr,
                        ]);
                      },
                    ),
                    const SizedBox(height: Space.lg),

                    RoutinesSection(
                      crewmateName: crewmate.name,
                      routines: crewmate.routines,
                      onAdd: () => _editRoutine(null),
                      onEdit: _editRoutine,
                      onToggle: _toggleRoutine,
                      onDelete: _deleteRoutine,
                    ),
                    const SizedBox(height: Space.lg),

                    ProfileGroup(
                      title: 'Activity',
                      children: [
                        ProfileRow(
                          icon: Icons.fact_check_outlined,
                          label: 'Audit',
                          onTap: () => _open(
                            CrewAuditScreen(
                              device: widget.device,
                              crewmateId: widget.crewmateId,
                              title: crewmate.name,
                            ),
                          ),
                        ),
                        ProfileRow(
                          icon: Icons.stacked_bar_chart_rounded,
                          label: 'Usage',
                          onTap: () => _open(
                            CrewmateUsageScreen(
                              device: widget.device,
                              crewmateId: widget.crewmateId,
                              name: crewmate.name,
                            ),
                          ),
                        ),
                      ],
                    ),

                    ProfileGroup(
                      title: 'Accounts',
                      children: [
                        Padding(
                          padding: const EdgeInsets.all(Space.md),
                          child: GithubCard(
                            status: _githubStatus,
                            flow: _login.flow,
                            loading: _githubLoading,
                            polling: _login.polling,
                            onConnect: _startLogin,
                            onDisconnect: _logout,
                            onCancelFlow: _login.cancel,
                          ),
                        ),
                      ],
                    ),

                    ProfileGroup(
                      children: [
                        ProfileRow(
                          icon: Icons.delete_outline_rounded,
                          label: 'Remove from the crew',
                          danger: true,
                          onTap: _deleteCrewmate,
                        ),
                      ],
                    ),
                  ],
                ),
                // Floating back button, like the chat's.
                Positioned(
                  top: MediaQuery.paddingOf(context).top + 2,
                  left: Space.md - 2,
                  child: Semantics(
                    button: true,
                    label: 'Back',
                    // The circle is drawn at 44; the hit area around it is the 48 dp a thumb needs.
                    child: Pressable(
                      onTap: () => Navigator.of(context).maybePop(),
                      child: SizedBox.square(
                        dimension: 48,
                        child: Center(
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
                  ),
                ),
              ],
            ),
    );
  }
}
