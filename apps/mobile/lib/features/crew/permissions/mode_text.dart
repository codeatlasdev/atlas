/// What a permission mode looks like and says: its icon, colour, name and one line, and the words for what hostd
/// refuses. Ask and Accept edits are neutral; Bypass, the only mode that runs what you did not look at, is the one
/// drawn in the warning colour.
library;

import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/connection/require_mux.dart';
import 'package:atlas_mobile/core/protocol/crew_ops_models.dart';
import 'package:atlas_mobile/core/protocol/permission_mode.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/l10n.dart';

extension PermissionModeLook on PermissionMode {
  /// Lock, pencil, shield: the same glyphs as Atlas Desktop.
  IconData get icon => switch (this) {
    PermissionMode.ask => Icons.lock_outline_rounded,
    PermissionMode.edits => Icons.edit_outlined,
    PermissionMode.bypass => Icons.shield_outlined,
  };

  /// Amber for Bypass only; the other two read as plain secondary text.
  Color color(AtlasColors c) =>
      this == PermissionMode.bypass ? c.warning : c.muted;

  String label(AppLocalizations l) => switch (this) {
    PermissionMode.ask => l.mobile_crew_mode_ask,
    PermissionMode.edits => l.mobile_crew_mode_edits,
    PermissionMode.bypass => l.mobile_crew_mode_bypass,
  };

  String detail(AppLocalizations l) => switch (this) {
    PermissionMode.ask => l.mobile_crew_mode_ask_detail,
    PermissionMode.edits => l.mobile_crew_mode_edits_detail,
    PermissionMode.bypass => l.mobile_crew_mode_bypass_detail,
  };
}

/// The words for a failure of `crewmate_set_approval` or `crewmate_create`: hostd's coded refusals in the app's
/// language, a phone paired without the `approve` scope, an unreachable computer, else the server's own text.
String modeFailureText(AppLocalizations l, Object error) {
  if (error is! HostRefusal) return errorText(error);
  if (error.missingApproveScope) return l.mobile_crew_mode_error_no_access;
  return switch (error.code) {
    approvalBypassDisabled => l.mobile_crew_mode_error_bypass_disabled,
    approvalForbidden => l.mobile_crew_mode_error_forbidden,
    approvalUnsupportedEngine => l.mobile_crew_mode_error_unsupported_engine(
      engineLabels['${error.args['engine']}'] ??
          '${error.args['engine'] ?? ''}',
    ),
    _ => l.mobile_crew_mode_error_generic(error.text),
  };
}

/// The words for a failure of `agent_approve`.
String approvalFailureText(AppLocalizations l, Object error) {
  if (error is! HostRefusal) return errorText(error);
  if (error.missingApproveScope) return l.mobile_crew_mode_error_no_access;
  return l.mobile_crew_approval_failed(error.text);
}
