import 'dart:convert';
import 'dart:io';
import 'package:flutter_test/flutter_test.dart';
import 'package:atlas_mobile/core/protocol/models.dart';

void main() {
  test('ShipGithubStatus parses bot_github_status fixture', () {
    final json = jsonDecode(
      File('test/fixtures/bot_github_status.json').readAsStringSync(),
    ) as Map<String, dynamic>;
    final status = ShipGithubStatus.fromJson(json);
    expect(status.installed, isTrue);
    expect(status.login, equals('alice'));
  });

  test('ShipGithubStatus handles null login', () {
    final status = ShipGithubStatus.fromJson({'installed': true, 'login': null});
    expect(status.login, isNull);
  });

  test('ShipLoginFlow parses bot_github_login_start response', () {
    final flow = ShipLoginFlow.fromJson({
      'user_code': 'ABCD-1234',
      'verification_uri': 'https://github.com/login/device',
    });
    expect(flow.userCode, equals('ABCD-1234'));
    expect(flow.verificationUri, equals('https://github.com/login/device'));
  });
}
