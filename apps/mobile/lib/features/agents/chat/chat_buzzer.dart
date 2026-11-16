import 'package:flutter/services.dart';

import 'package:atlas_mobile/core/protocol/agent_tree.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/features/agents/agent_info.dart';

/// The haptics of an open agent chat: a tick when the main agent ends a turn, a heavy one
/// when it needs the user or a subagent fails, a light one when a subagent finishes.
/// The first read of each only records, so opening the screen never buzzes for old news.
class ChatBuzzer {
  /// Last seen status per node.
  final _seen = <String, String>{};
  String? _seenState;

  /// The main agent itself ended a turn or needs the user.
  void onStatus(AgentStatus status) {
    if (_seenState != null && _seenState != status.state) {
      if (status.state == 'done') {
        HapticFeedback.lightImpact();
      } else if (needsYou(status)) {
        HapticFeedback.heavyImpact();
      }
    }
    _seenState = status.state;
  }

  /// A tick when a node ends well, a heavier one when it fails.
  void onTree(AgentTree t) {
    final first = _seen.isEmpty;
    var failed = false, done = false;
    for (final n in t.nodes) {
      final before = _seen[n.id];
      if (!first && before != null && before != n.status) {
        if (n.failed) failed = true;
        if (n.done && !n.isRoot) done = true;
      }
      _seen[n.id] = n.status;
    }
    if (failed) {
      HapticFeedback.heavyImpact();
    } else if (done) {
      HapticFeedback.lightImpact();
    }
  }
}
