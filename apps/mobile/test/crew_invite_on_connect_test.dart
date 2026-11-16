import 'package:material_ui/material_ui.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/features/crew/list/crew_screen.dart';

import 'support/demo_harness.dart';

// An invite made while the phone was off (`atlas crew pair` is not one, but the desktop's "add crew server" is)
// is offered when the Crew tab opens: the phone asks `mobile_invites` on connect instead of waiting for a push
// that already went by. PhoneCrewAudit gaps 1 and 4.

void main() {
  const mac = MacDevice(nodeId: 'mac-1', name: 'Mac');

  testWidgets(
    'a pending invite no push announced opens the add-crew-server sheet',
    (t) async {
      await t.binding.setSurfaceSize(const Size(393, 852));
      // The demo computer holds one pending invite from the start and never pushes it.
      await t.pumpWidget(demoWrap(const CrewScreen(), [mac]));
      await settle(t);
      expect(find.text('New crew server'), findsOneWidget);
      expect(find.textContaining('myserver'), findsOneWidget);
      await t.pumpWidget(const SizedBox());
      await settle(t);
    },
  );
}
