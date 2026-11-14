import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/features/crew/list/crew_screen.dart';
import 'package:atlas_mobile/features/crew/chat/crewmate_chat_screen.dart';

import 'support/demo_harness.dart';

void main() {
  const mac = MacDevice(nodeId: 'demo', name: 'Demo MacBook');

  testWidgets('Crew tab lists demo crewmate', (t) async {
    await t.binding.setSurfaceSize(const Size(393, 852));
    await t.pumpWidget(demoWrap(const CrewScreen(), [mac]));
    await settle(t);
    // The demo computer has a "Reviewer" crewmate.
    expect(find.text('Reviewer'), findsWidgets);
    await t.pumpWidget(const SizedBox());
    await settle(t);
  });

  testWidgets('Crew tab empty state without paired computers', (t) async {
    await t.binding.setSurfaceSize(const Size(393, 852));
    await t.pumpWidget(demoWrap(const CrewScreen(), []));
    await settle(t);
    // No devices paired → the screen says to pair a computer first.
    expect(find.textContaining('No crew yet'), findsOneWidget);
    await t.pumpWidget(const SizedBox());
    await settle(t);
  });

  testWidgets('CrewmateChatScreen: send task shows queued then done', (
    t,
  ) async {
    await t.binding.setSurfaceSize(const Size(393, 852));
    await t.pumpWidget(
      demoWrap(CrewmateChatScreen(device: mac, crewmateId: 'crewmate-1'), [
        mac,
      ]),
    );
    await settle(t);
    // Nothing to send: there is no send button, the field has the row.
    expect(find.bySemanticsLabel('Send'), findsNothing);
    final field = find.byType(TextField);
    final emptyWidth = t.getSize(field).width;
    await t.enterText(field, 'Review this PR');
    await settle(t);
    // Text in: the button has come out and the field gave up room for it.
    expect(find.bySemanticsLabel('Send'), findsOneWidget);
    expect(t.getSize(field).width, lessThan(emptyWidth));
    await t.tap(find.bySemanticsLabel('Send'));
    await settle(t);
    // Task prompt is visible in the task list.
    expect(find.text('Review this PR'), findsOneWidget);
    // Wait for demo task to complete (~1.8 s total).
    for (var i = 0; i < 25; i++) {
      await t.pump(const Duration(milliseconds: 100));
    }
    // The demo result contains "demo".
    expect(find.textContaining('demo'), findsWidgets);
    await t.pumpWidget(const SizedBox());
    await settle(t);
  });

  testWidgets(
    'CrewmateChatScreen: clearing the text takes the send button back',
    (t) async {
      await t.binding.setSurfaceSize(const Size(393, 852));
      await t.pumpWidget(
        demoWrap(CrewmateChatScreen(device: mac, crewmateId: 'crewmate-1'), [
          mac,
        ]),
      );
      await settle(t);
      final field = find.byType(TextField);
      final emptyWidth = t.getSize(field).width;
      await t.enterText(field, 'x');
      await settle(t);
      await t.enterText(field, '   ');
      await settle(t);
      // Only spaces is nothing to send.
      expect(find.bySemanticsLabel('Send'), findsNothing);
      expect(t.getSize(field).width, emptyWidth);
      await t.pumpWidget(const SizedBox());
      await settle(t);
    },
  );

  testWidgets('Crew tab shows crewmate needing approval in inbox', (t) async {
    await t.binding.setSurfaceSize(const Size(393, 852));
    // demoDevice connects and demo computer returns a crewmate with pending_approval.
    await t.pumpWidget(demoWrap(const CrewScreen(), [mac]));
    await settle(t);
    // Demo crewmate has pending approval → inbox shows it.
    expect(find.text('Deny'), findsOneWidget);
    await t.pumpWidget(const SizedBox());
    await settle(t);
  });
}
