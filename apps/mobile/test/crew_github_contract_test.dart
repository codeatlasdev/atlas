import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:atlas_mobile/core/protocol/crew_models.dart';

void main() {
  test('CrewGithubStatus parses crew_github_status fixture', () {
    final json = jsonDecode(
      File('test/fixtures/crew_github_status.json').readAsStringSync(),
    ) as Map<String, dynamic>;
    final status = CrewGithubStatus.fromJson(json);
    expect(status.installed, isTrue);
    expect(status.login, equals('alice'));
  });

  test('CrewGithubStatus handles null login', () {
    final status = CrewGithubStatus.fromJson({
      'installed': true,
      'login': null,
    });
    expect(status.login, isNull);
  });

  test('CrewGithubLogin parses crew_github_login_start response', () {
    final flow = CrewGithubLogin.fromJson({
      'user_code': 'ABCD-1234',
      'verification_uri': 'https://github.com/login/device',
    });
    expect(flow.userCode, equals('ABCD-1234'));
    expect(flow.verificationUri, equals('https://github.com/login/device'));
  });
}
