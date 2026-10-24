// Regression: BotListEntry.runningTask was parsed as String? but the real
// producer (crates/atlas-host/src/bots/mod.rs BotList) emits an object
// {"id": "...", "pane": "..."}.  This test consumes the fixture written by
// the Rust producer test (bots/tests.rs bot_list_json_contract) — a REAL
// daemon reply with volatile fields pinned to stable values.
import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/protocol/models.dart';

void main() {
  late List<dynamic> bots;

  setUpAll(() {
    final raw = File('test/fixtures/bot_list.json').readAsStringSync();
    final fixture = jsonDecode(raw) as Map<String, dynamic>;
    bots = fixture['bots'] as List<dynamic>;
  });

  group('BotListEntry.fromJson — real Rust bot_list payload', () {
    // ── working bot: running_task is an OBJECT {id, pane} ────────────────────
    test('parses working bot: running_task is BotRunningTask, not null/String', () {
      final raw = bots.firstWhere((b) => (b as Map)['state'] == 'working')
          as Map<String, dynamic>;
      final entry = BotListEntry.fromJson(raw);

      expect(entry.state, 'working');
      expect(entry.agent, 'claude');
      expect(entry.queued, 0);

      // THE BUG: runningTask was String? and `j['running_task'] as String?`
      // returned null for an object, crashing callers that used the pane key.
      // After the fix, runningTask must be a BotRunningTask with id and pane.
      expect(
        entry.runningTask,
        isNotNull,
        reason: 'runningTask must parse the {id,pane} object — not return null',
      );
      expect(entry.runningTask!.id, 'task-working');
      expect(entry.runningTask!.pane, 'bot:bot-working:task-working');
    });

    // ── working bot: last field present ──────────────────────────────────────
    test('parses last field on working bot', () {
      final raw = bots.firstWhere((b) => (b as Map)['state'] == 'working')
          as Map<String, dynamic>;
      final entry = BotListEntry.fromJson(raw);

      expect(entry.last, isNotNull);
      expect(entry.last!.text, isNotEmpty);
      expect(entry.last!.ts, isPositive);
    });

    // ── idle bot with done task: last present, no running_task ────────────────
    test('parses idle bot: last present, running_task absent', () {
      final raw = bots.firstWhere(
              (b) => (b as Map)['id'] == 'bot-done') as Map<String, dynamic>;
      final entry = BotListEntry.fromJson(raw);

      expect(entry.id, 'bot-done');
      expect(entry.state, 'idle');
      expect(entry.last, isNotNull);
      expect(entry.last!.text, 'nightly done');
      expect(entry.last!.ts, 1700000000);
      expect(entry.runningTask, isNull);
      expect(entry.queued, 0);
      // No engine key → approvalSupported defaults to true.
      expect(entry.approvalSupported, isTrue,
          reason: 'claude bot without engine key defaults approvalSupported to true');
    });

    // ── opencode bot: approval_supported:false ───────────────────────────────
    test('parses opencode bot: approvalSupported is false', () {
      final raw = bots.firstWhere(
              (b) => (b as Map)['id'] == 'bot-opencode') as Map<String, dynamic>;
      final entry = BotListEntry.fromJson(raw);

      expect(entry.id, 'bot-opencode');
      expect(entry.agent, 'opencode');
      expect(entry.state, 'idle');
      // Real Rust producer emits approval_supported:false for opencode bots.
      expect(entry.approvalSupported, isFalse,
          reason: 'opencode bot must have approvalSupported=false from the real producer');
      expect(entry.runningTask, isNull);
    });
  });
}
