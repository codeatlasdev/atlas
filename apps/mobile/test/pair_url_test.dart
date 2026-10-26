import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/protocol/models.dart';

void main() {
  group('PairInfo.parse', () {
    // The URL Settings → Integrations → Mobile really shows (fixtures/pair_url.txt, written by the
    // desktop's mobile.test.ts from hostd's real `mobile_pair_start` reply): a hand-typed
    // URL here could hide an encoding the phone reads differently ('+' for a space).
    test('reads the pairing URL the desktop produces', () {
      final url = File('test/fixtures/pair_url.txt').readAsStringSync().trim();
      final start = jsonDecode(
        File('../desktop/src/lib/__fixtures__/mobile-pair-start.json')
            .readAsStringSync(),
      ) as Map<String, dynamic>;
      final info = PairInfo.parse(url);
      expect(info.nodeId, start['node_id']);
      expect(info.code, start['code']);
      expect(info.name, start['hostname']);
      expect(info.addrs, start['addrs']);
      expect(info.relay, start['relay_url']);
    });

    test('parses a valid full URL', () {
      const url =
          'atlas-mobile://pair?node=abc123&code=654321&name=MyMac'
          '&addr=1.2.3.4:1234&addr=5.6.7.8:5678&relay=https://relay.example.com';
      final info = PairInfo.parse(url);
      expect(info.nodeId, 'abc123');
      expect(info.code, '654321');
      expect(info.name, 'MyMac');
      expect(info.addrs, ['1.2.3.4:1234', '5.6.7.8:5678']);
      expect(info.relay, 'https://relay.example.com');
    });

    test('parses a minimal URL (no addrs, no relay)', () {
      const url = 'atlas-mobile://pair?node=n1&code=000000&name=Mac';
      final info = PairInfo.parse(url);
      expect(info.nodeId, 'n1');
      expect(info.code, '000000');
      expect(info.addrs, isEmpty);
      expect(info.relay, isNull);
    });

    test('throws ArgumentError on bad scheme', () {
      expect(
        () => PairInfo.parse('https://pair?node=x&code=1'),
        throwsA(isA<ArgumentError>()),
      );
    });

    test('throws ArgumentError when host is not "pair"', () {
      expect(
        () => PairInfo.parse('atlas-mobile://connect?node=x&code=1'),
        throwsA(isA<ArgumentError>()),
      );
    });

    test('throws ArgumentError when node param is missing', () {
      expect(
        () => PairInfo.parse('atlas-mobile://pair?code=123456'),
        throwsA(isA<ArgumentError>()),
      );
    });

    test('throws ArgumentError when code param is missing', () {
      expect(
        () => PairInfo.parse('atlas-mobile://pair?node=abc'),
        throwsA(isA<ArgumentError>()),
      );
    });

    test('keeps repeated addr values without deduplication', () {
      const url =
          'atlas-mobile://pair?node=n&code=1&addr=1.1.1.1:1&addr=1.1.1.1:1';
      final info = PairInfo.parse(url);
      // Duplicates are preserved per contract.
      expect(info.addrs.length, 2);
    });
  });
}
