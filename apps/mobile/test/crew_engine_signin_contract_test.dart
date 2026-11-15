// Contract: the phone's engine sign-in models parse the REAL replies of hostd (`crew_engine_status`,
// `crew_engine_login`), written by its serializer from a real daemon with fake engine CLIs, never by hand:
//   UPDATE_FIXTURES=1 cargo test -p atlas-host --lib -- crew::tests::engine_login
// Incident (PhoneCrewAudit gap 2): the action card said "Sign in to claude in Settings" and the phone had no way to.
import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/protocol/crew_ops_models.dart';

Map<String, dynamic> _fixture(String name) =>
    jsonDecode(File('test/fixtures/$name').readAsStringSync())
        as Map<String, dynamic>;

void main() {
  group('crew_engine_status — the real reply', () {
    final rows = parseEngineStatus(_fixture('crew_engine_status.json'));

    test('every engine hostd can sign in has a row, in the settings order', () {
      expect(rows.map((r) => r.engine), engineLabels.keys.toList());
    });

    test('hostd\'s states read as the rows the settings screen shows', () {
      final by = {for (final r in rows) r.engine: r};
      expect(by['claude']!.state, 'signed_in');
      expect(by['claude']!.text, 'Signed in');
      expect(by['claude']!.needsSignin, isFalse);
      // codex's `login status` exits 1 ("Not logged in"): the row offers the sign-in.
      expect(by['codex']!.state, 'needs_signin');
      expect(by['codex']!.text, 'Needs sign-in');
      expect(by['codex']!.needsSignin, isTrue);
      // Not on the server's PATH: nothing to sign in.
      expect(by['omp']!.state, 'not_installed');
      expect(by['opencode']!.text, 'Not installed on this server');
    });

    test('an engine name the phone does not know still parses, last', () {
      final withNew = parseEngineStatus({
        'engines': [
          {'engine': 'newagent', 'state': 'needs_signin'},
          ...(_fixture('crew_engine_status.json')['engines'] as List),
        ],
      });
      expect(withNew.last.engine, 'newagent');
      expect(withNew.last.label, 'newagent');
    });
  });

  group('crew_engine_login — the real reply', () {
    test('names the engine and the terminal to open', () {
      final l = EngineLogin.fromJson(_fixture('crew_engine_login.json'));
      expect(l.engine, 'codex');
      expect(l.pty, isNotEmpty);
    });
  });

  group('the sign-in audit entry hostd writes', () {
    // Shaped like hostd's `Audit::record(None, "engine_login_started", {engine, by})` line.
    AuditEntry entry(String by) => parseAudit({
      'entries': [
        {
          'seq': 1,
          'ts': 1760000000,
          'kind': 'engine_login_started',
          'engine': 'codex',
          'by': by,
        },
      ],
    }).single;

    test('reads as a sentence with the engine and who asked, in Settings', () {
      expect(
        entry('phone:abcdef0123456789').summary,
        'Sign-in of codex started (from phone abcdef01…)',
      );
      expect(
        entry('desktop').summary,
        'Sign-in of codex started (from desktop)',
      );
      expect(entry('desktop').group, AuditGroup.config);
      expect(auditKindLabel('engine_login_started'), 'Sign-in started');
    });
  });
}
