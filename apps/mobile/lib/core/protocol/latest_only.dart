import 'package:flutter/widgets.dart';

/// "Only the newest answer counts" for a screen that asks the same question again and again (a poll,
/// a search box).
///
/// A slow link answers out of order: the reply to the question asked first can land after the one
/// asked last, and writing it to the screen rolls the screen back. Two rules fix that, and this holds
/// both so a screen does not re-invent them:
///
/// - **Latest wins.** [begin] hands out a [Ticket]; only the newest ticket is [Ticket.isCurrent]. A
///   screen applies a reply only when its ticket is current.
/// - **One poll at a time.** [tryBegin] refuses while a [Ticket] it handed out is still unfinished,
///   so a timer that ticks faster than the link answers puts ONE request on the wire, not a queue.
///
/// A [Ticket] is finished with [Ticket.done] (always, in a `finally`). [close] (in `dispose`) makes
/// every ticket stale at once.
final class LatestOnly {
  int _seq = 0;
  int _open = 0;
  bool _closed = false;

  /// A new question that replaces every earlier one: older tickets stop being current. Use it for a
  /// user-driven load (a search, a filter), which must run even if a poll is in flight.
  Ticket begin() {
    _open++;
    return Ticket._(this, ++_seq);
  }

  /// A new question for a timer, or null while an earlier one is still in flight (or after [close]).
  Ticket? tryBegin() => _closed || _open > 0 ? null : begin();

  /// After this nothing is current and [tryBegin] hands out nothing. Call from `dispose`.
  void close() => _closed = true;
}

/// One question asked through a [LatestOnly].
final class Ticket {
  Ticket._(this._owner, this._seq);
  final LatestOnly _owner;
  final int _seq;
  bool _done = false;

  /// True while no newer question was asked and the owner is open: the reply may be applied.
  bool get isCurrent => !_owner._closed && _seq == _owner._seq;

  /// The reply (or failure) arrived; lets [LatestOnly.tryBegin] hand out the next one. Idempotent.
  void done() {
    if (_done) return;
    _done = true;
    _owner._open--;
  }
}

/// False while the app is in the background (hidden, paused, inactive, detached): a poll tick skips
/// then instead of hitting a link the phone is about to suspend. Null (no frame yet, a test) counts
/// as foreground. Read at tick time, so there is no observer to register or forget to remove.
bool appInForeground() {
  final s = WidgetsBinding.instance.lifecycleState;
  return s == null || s == AppLifecycleState.resumed;
}
