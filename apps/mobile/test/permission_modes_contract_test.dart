// Contract: the phone's permission-mode models read the REAL payloads of hostd (written by its serializer, never by
// hand): crew_permission_modes.json, crewmate_list.json and crew_settings*.json.
//   UPDATE_FIXTURES=1 cargo test -p atlas-host crew::tests::permissions
// Incident this guards: the phone knew `approval: ask | auto`; hostd now decides every tool call by `mode`
// (ask, edits, bypass), keeps `approval` only as a reading aid, and codes its refusals.
import 'dart:convert';
import 'dart:io';

import 'package:flutter/widgets.dart' show Locale;
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/protocol/crew_models.dart';
import 'package:atlas_mobile/core/protocol/crew_ops_models.dart';
import 'package:atlas_mobile/core/protocol/permission_mode.dart';
import 'package:atlas_mobile/features/crew/crew_provider.dart';
import 'package:atlas_mobile/features/crew/permissions/mode_text.dart';
import 'package:atlas_mobile/l10n/app_localizations.dart';

Map<String, dynamic> _fixture(String name) =>
    jsonDecode(File('test/fixtures/$name').readAsStringSync())
        as Map<String, dynamic>;

Map<String, dynamic> _modes(String key) =>
    (_fixture('crew_permission_modes.json')[key] as Map).cast();

final _en = lookupAppLocalizations(const Locale('en'));
final _pt = lookupAppLocalizations(const Locale('pt'));

void main() {
  group('the mode of a crewmate — real crewmate payloads', () {
    test('Bypass, Accept edits and an engine with no channel', () {
      final bypass = CrewmateEntry.fromJson(_modes('crewmate_bypass'));
      expect(bypass.mode, PermissionMode.bypass);
      expect(bypass.approvalSupported, isTrue);

      final edits = CrewmateEntry.fromJson(_modes('crewmate_edits'));
      expect(edits.mode, PermissionMode.edits);
      expect(edits.approvalSupported, isTrue);

      final codex = CrewmateEntry.fromJson(_modes('crewmate_unsupported'));
      expect(
        codex.mode,
        PermissionMode.bypass,
        reason: 'it always runs unattended',
      );
      expect(codex.approvalSupported, isFalse);
    });

    test(
      'the reply of crewmate_set_approval is the crewmate in its new mode',
      () {
        final now = CrewmateEntry.fromJson(_modes('set_approval'));
        expect(now.mode, PermissionMode.edits);
        expect(now.id, 'crewmate-fixture');
      },
    );

    test('the crew list carries a mode per crewmate', () {
      final list = parseCrewList(_fixture('crewmate_list.json'), 'x');
      final by = {for (final c in list.crewmates) c.id: c};
      expect(by['crewmate-working']!.mode, PermissionMode.ask);
      expect(by['crewmate-done']!.mode, PermissionMode.bypass);
      expect(by['crewmate-opencode']!.mode, PermissionMode.bypass);
      expect(by['crewmate-opencode']!.approvalSupported, isFalse);
    });

    test(
      'an older hostd without `mode` is read from `approval` (auto was Bypass)',
      () {
        expect(
          PermissionMode.ofEntry(const {'approval': 'auto'}),
          PermissionMode.bypass,
        );
        expect(
          PermissionMode.ofEntry(const {'approval': 'ask'}),
          PermissionMode.ask,
        );
        expect(PermissionMode.ofEntry(const {}), PermissionMode.ask);
      },
    );

    test(
      'a mode this build does not know falls back to what `approval` says',
      () {
        expect(
          PermissionMode.ofEntry(const {'mode': 'plan', 'approval': 'ask'}),
          PermissionMode.ask,
        );
      },
    );
  });

  group('a pending approval — real agent_approval payload', () {
    test(
      'keeps the rail that forced the question, and the task it belongs to',
      () {
        final pa = CrewmatePendingApproval.fromJson(_modes('pending_approval'));
        expect(pa.rail, ApprovalRail.protectedPath);
        expect(pa.taskId, 'task-fixture');
        expect(pa.toolName, 'Write');
        expect(pa.req, 'req-fixture');
      },
    );

    test(
      'one without a rail has none; an unknown rail is none, not a crash',
      () {
        expect(
          CrewmatePendingApproval.fromJson(const {
            'req': 'r',
            'pane_key': 'crew:a:b',
          }).rail,
          isNull,
        );
        expect(
          CrewmatePendingApproval.fromJson(const {
            'req': 'r',
            'rail': 'new_rail',
          }).rail,
          isNull,
        );
        expect(
          CrewmatePendingApproval.fromJson(const {
            'req': 'r',
            'rail': 'no_auto',
          }).rail,
          ApprovalRail.noAuto,
        );
      },
    );
  });

  group('crew settings — real crew_settings_get reply', () {
    test('default mode and Allow Bypass', () {
      final s = CrewSettings.fromJson(_fixture('crew_settings.json'));
      expect(s.defaultMode, PermissionMode.ask);
      expect(s.allowBypass, isTrue);
      final off = CrewSettings.fromJson(_modes('settings_no_bypass'));
      expect(off.allowBypass, isFalse);
      expect(off.defaultMode, PermissionMode.ask);
    });

    test('an older hostd without the fields: Ask, Bypass allowed', () {
      final s = CrewSettings.fromJson(const {'short_replies': true});
      expect(s.defaultMode, PermissionMode.ask);
      expect(s.allowBypass, isTrue);
    });

    test('a save sends both fields', () {
      final args = CrewSettings.fromJson(_modes('settings_no_bypass')).toArgs();
      expect(args['allow_bypass'], isFalse);
      expect(args['default_approval'], 'ask');
    });
  });

  group('coded refusals — the real error reply', () {
    test('crew.approval.bypass_disabled keeps its code and is said in both languages', () {
      final reply = {
        'ok': false,
        'error': 'bypass is not allowed on this crew',
        ..._modes('refusal'),
      };
      final r = HostRefusal.fromReply(reply);
      expect(r.code, approvalBypassDisabled);
      expect(
        modeFailureText(_en, r),
        _en.mobile_crew_mode_error_bypass_disabled,
      );
      expect(
        modeFailureText(_pt, r),
        _pt.mobile_crew_mode_error_bypass_disabled,
      );
      expect(modeFailureText(_en, r), isNot(modeFailureText(_pt, r)));
    });

    test('unsupported_engine names the engine as the phone labels it', () {
      final r = HostRefusal.fromReply(const {
        'ok': false,
        'error': 'codex has no approval channel: it always runs unattended',
        'code': approvalUnsupportedEngine,
        'args': {'engine': 'codex', 'mode': 'ask'},
      });
      expect(modeFailureText(_en, r), contains('Codex'));
      expect(modeFailureText(_en, r), contains('Only Bypass applies'));
    });

    test('forbidden (a crewmate token) and a phone paired without the approve scope', () {
      final forbidden = HostRefusal.fromReply(const {
        'ok': false,
        'error': 'only the owner can change a permission mode',
        'code': approvalForbidden,
        'args': {},
      });
      expect(
        modeFailureText(_en, forbidden),
        _en.mobile_crew_mode_error_forbidden,
      );
      final noScope = HostRefusal.fromReply(const {
        'ok': false,
        'error': 'forbidden: approve',
      });
      expect(noScope.missingApproveScope, isTrue);
      expect(
        modeFailureText(_en, noScope),
        _en.mobile_crew_mode_error_no_access,
      );
      expect(
        approvalFailureText(_en, noScope),
        _en.mobile_crew_mode_error_no_access,
      );
    });

    test('an uncoded refusal shows hostd\'s own text', () {
      final r = HostRefusal.fromReply(const {
        'ok': false,
        'error': 'crewmate x not found',
      });
      expect(r.code, isNull);
      expect(
        modeFailureText(_en, r),
        _en.mobile_crew_mode_error_generic('crewmate x not found'),
      );
      expect(
        approvalFailureText(_en, r),
        _en.mobile_crew_approval_failed('crewmate x not found'),
      );
    });
  });

  group('the answer to a prompt — agent_approve arguments', () {
    Map<String, Object?> args({
      bool allow = true,
      ApprovalScope scope = ApprovalScope.once,
      ApprovalRail? rail,
      String? comment,
    }) => approvalAnswerArgs(
      pane: 'crew:c:t',
      req: 'r1',
      allow: allow,
      scope: scope,
      rail: rail,
      comment: comment,
    );

    test('Allow once is the default: no scope, no comment', () {
      expect(args(), {'pane': 'crew:c:t', 'req': 'r1', 'allow': true});
    });

    test('Allow for this task sends scope task', () {
      expect(args(scope: ApprovalScope.task)['scope'], 'task');
    });

    test(
      'a call a rail held can only be allowed once: scope is never sent for it',
      () {
        expect(
          args(scope: ApprovalScope.task, rail: ApprovalRail.protectedPath),
          isNot(contains('scope')),
        );
      },
    );

    test('Deny never carries a scope, and keeps its comment', () {
      final a = args(
        allow: false,
        scope: ApprovalScope.task,
        comment: '  use the staging db  ',
      );
      expect(a, {
        'pane': 'crew:c:t',
        'req': 'r1',
        'allow': false,
        'comment': 'use the staging db',
      });
    });

    test(
      'a blank comment is not sent; one over 2000 characters is cut to 2000',
      () {
        expect(args(comment: '   '), isNot(contains('comment')));
        expect(
          (args(comment: 'x' * 2500)['comment']! as String).length,
          approvalCommentMax,
        );
      },
    );
  });

  group('the audit of a decision and of a switch — real entries', () {
    test('a decision names the tool, who answered and how', () {
      final e = AuditEntry.fromJson(_modes('audit_approval_decision'));
      expect(e.summary, 'Allowed Write (answered by desktop)');
      expect(e.summaryIn(_pt), 'Permitido: Write (respondido por desktop)');
    });

    test('a decision by the mode, by a grant, by a rail and by timeout', () {
      AuditEntry decision(Map<String, Object?> extra) => AuditEntry.fromJson({
        'seq': 1,
        'ts': 1,
        'kind': 'approval_decision',
        'tool': 'Bash',
        'decision': 'allow',
        ...extra,
      });
      expect(
        decision({'source': 'mode'}).summary,
        'Allowed Bash (by the mode)',
      );
      expect(
        decision({'source': 'grant'}).summary,
        'Allowed Bash (by a task grant)',
      );
      expect(
        decision({'source': 'rail', 'decision': 'deny'}).summary,
        'Denied Bash (held, nobody could answer)',
      );
      expect(
        decision({'source': 'timeout', 'decision': 'deny'}).summary,
        'Timed out Bash (nobody answered)',
      );
    });

    test('a mode change names both modes and who switched', () {
      final e = AuditEntry.fromJson(_modes('audit_approval_mode_changed'));
      expect(
        e.summary,
        'Mode changed from Accept edits to Bypass (answered by desktop)',
      );
      expect(
        e.summaryIn(_pt),
        'Modo alterado de Aceitar edições para Bypass (respondido por desktop)',
      );
      expect(auditKindLabelIn(_en, e.kind), 'Mode changed');
      expect(auditKindLabelIn(_pt, e.kind), 'Modo alterado');
      expect(e.group, AuditGroup.approvals);
    });
  });
}
