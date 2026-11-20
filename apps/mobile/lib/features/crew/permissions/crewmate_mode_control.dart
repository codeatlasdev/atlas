/// A crewmate's permission mode, switchable: the picker wired to `crewmate_set_approval`, with the coded refusals
/// said in the app's language and the live value (a change made on the desktop or Telegram arrives through
/// `crewmate_updated` and redraws it). Inside the chat's sheet and the crewmate's profile.
library;

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/protocol/crew_models.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/protocol/permission_mode.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/l10n.dart';
import 'package:atlas_mobile/features/crew/crew_provider.dart';
import 'package:atlas_mobile/features/crew/ops/crew_ops_provider.dart';
import 'package:atlas_mobile/features/crew/permissions/mode_picker.dart';
import 'package:atlas_mobile/features/crew/permissions/mode_text.dart';

class CrewmateModeControl extends ConsumerStatefulWidget {
  const CrewmateModeControl({
    super.key,
    required this.device,
    required this.crewmate,
    this.onChanged,
  });

  final MacDevice device;
  final CrewmateEntry crewmate;

  /// Called with the crewmate as hostd now shows it, after a switch it accepted.
  final ValueChanged<CrewmateEntry>? onChanged;

  @override
  ConsumerState<CrewmateModeControl> createState() =>
      _CrewmateModeControlState();
}

class _CrewmateModeControlState extends ConsumerState<CrewmateModeControl> {
  bool _busy = false;
  String? _error;

  Future<void> _pick(PermissionMode mode) async {
    final l = context.l10n;
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final now = await setCrewmateMode(
        ref,
        widget.device,
        widget.crewmate.id,
        mode,
      );
      if (mounted) widget.onChanged?.call(now);
    } catch (e) {
      if (mounted) setState(() => _error = modeFailureText(l, e));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final l = context.l10n;
    final crewmate = widget.crewmate;
    // Until the crew's settings arrive Bypass is offered: hostd has the last word and says why it refuses.
    final allowBypass =
        ref.watch(crewSettingsProvider(widget.device)).value?.allowBypass ??
        true;
    final fixed = !crewmate.approvalSupported;
    // While a switch is in flight the rows ignore taps (a no-op callback keeps them drawn as live).
    final ValueChanged<PermissionMode>? onPick = fixed
        ? null
        : _busy
        ? (_) {}
        : _pick;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        ModePicker(
          value: crewmate.mode,
          allowBypass: allowBypass,
          onChanged: onPick,
          reason: fixed ? l.mobile_crew_mode_no_channel : null,
        ),
        if (_error != null)
          Semantics(
            liveRegion: true,
            child: Padding(
              padding: const EdgeInsets.only(top: Space.sm),
              child: Text(
                _error!,
                style: Theme.of(context).textTheme.bodyMedium!
                    .copyWith(color: c.error, height: 1.4),
              ),
            ),
          ),
      ],
    );
  }
}

/// The chat's mode sheet: who it is for, the picker, and a note that a change applies from the next tool call while
/// the crewmate is working. A switch closes it and says the new mode in a snackbar, like Telegram's `/mode`.
Future<void> showModeSheet(
  BuildContext context, {
  required MacDevice device,
  required String crewmateId,
}) {
  final messenger = ScaffoldMessenger.of(context);
  return showModalBottomSheet<void>(
    context: context,
    isScrollControlled: true,
    useSafeArea: true,
    showDragHandle: true,
    builder: (sheet) => _ModeSheet(
      device: device,
      crewmateId: crewmateId,
      onChanged: (c) {
        Navigator.of(sheet).pop();
        final l = sheet.l10n;
        messenger.showSnackBar(
          SnackBar(
            content: Text(l.mobile_crew_mode_changed(c.name, c.mode.label(l))),
          ),
        );
      },
    ),
  );
}

class _ModeSheet extends ConsumerWidget {
  const _ModeSheet({
    required this.device,
    required this.crewmateId,
    required this.onChanged,
  });

  final MacDevice device;
  final String crewmateId;
  final ValueChanged<CrewmateEntry> onChanged;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final c = AtlasColors.of(context);
    final l = context.l10n;
    final crewmate = ref
        .watch(crewProvider(device))
        .value
        ?.crewmates
        .where((b) => b.id == crewmateId)
        .firstOrNull;
    if (crewmate == null) return const SizedBox(height: 120);
    final running = crewmate.state != 'idle';
    return SingleChildScrollView(
      padding: EdgeInsets.fromLTRB(
        Space.xl,
        Space.sm,
        Space.xl,
        Space.xl + MediaQuery.viewInsetsOf(context).bottom,
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Semantics(
            header: true,
            child: Text(
              l.mobile_crew_mode_sheet_title(crewmate.name),
              style: Theme.of(context).textTheme.titleLarge,
            ),
          ),
          const SizedBox(height: Space.lg),
          CrewmateModeControl(
            device: device,
            crewmate: crewmate,
            onChanged: onChanged,
          ),
          if (running && crewmate.approvalSupported)
            Padding(
              padding: const EdgeInsets.only(top: Space.sm),
              child: Text(
                l.mobile_crew_mode_applies_next,
                style: Theme.of(context).textTheme.bodyMedium!
                    .copyWith(color: c.muted, height: 1.4),
              ),
            ),
        ],
      ),
    );
  }
}
