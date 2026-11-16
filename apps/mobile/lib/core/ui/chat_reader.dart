import 'package:material_ui/material_ui.dart';

import 'package:atlas_mobile/core/ui/atlas_theme.dart';

/// What happens when something arrives in a chat while the reader is above its end: one behaviour for every chat.
///
/// A chat is a reversed list, newest at the bottom. Anything added or changed near the end would move the part the
/// reader is on by its height, in one frame. So while they read above the end the thread stays as it was (the
/// reader keeps the snapshot of what it showed when they scrolled away), and a "New activity" button says there is
/// more. Back at the end (scrolled there, or by that button) they get the live thread again.
///
/// The screen owns one, gives its [scroll] to its list and calls [view] on every build:
///
/// ```dart
/// final shown = _reader.view(thread, thread.length); // the list draws `shown`
/// if (_reader.hasNew) NewActivityButton(onPressed: _reader.jumpToEnd)
/// ```
class ChatReader<T> {
  /// [onChanged] rebuilds the screen: the reader moved away from the end or came back to it.
  ChatReader({required this.onChanged}) {
    scroll.addListener(_onScroll);
  }

  final VoidCallback onChanged;

  /// The list's controller.
  final ScrollController scroll = ScrollController();

  /// Within this many pixels of the newest message the reader counts as being at the end.
  static const _endSlack = 80.0;

  bool _atEnd = true;

  /// What the thread showed when the reader scrolled away from the end; null while at the end.
  T? _frozen;

  /// What the last build would show now.
  T? _live;

  /// Messages in the thread when the reader last sat at the end; more now = new activity.
  int _seen = 0;
  bool _hasNew = false;

  /// Something arrived below the reader since they left the end.
  bool get hasNew => _hasNew;

  /// The thread to draw: [live] at the end, else the one the reader left. [count] is how many messages [live] has.
  T view(T live, int count) {
    _live = live;
    if (_atEnd) _seen = count;
    _hasNew = !_atEnd && count > _seen;
    return _frozen ?? live;
  }

  void _onScroll() {
    final at = !scroll.hasClients || scroll.offset < _endSlack;
    if (at == _atEnd) return;
    _atEnd = at;
    _frozen = at ? null : _live;
    onChanged();
  }

  /// A new thread (another agent was focused): the reader is at its end, nothing is new. The caller rebuilds.
  void reset() {
    _atEnd = true;
    _frozen = null;
    _seen = 0;
  }

  /// Scrolls to the newest message.
  void jumpToEnd() {
    if (!scroll.hasClients) return;
    scroll.animateTo(0, duration: Motion.slow, curve: Motion.ease);
  }

  void dispose() => scroll.dispose();
}

/// The button a [ChatReader] with [ChatReader.hasNew] shows: tap to go to the end. The screen places it.
class NewActivityButton extends StatelessWidget {
  const NewActivityButton({super.key, required this.onPressed});

  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) => FilledButton.tonalIcon(
    onPressed: onPressed,
    icon: const Icon(Icons.arrow_downward, size: 16),
    label: const Text('New activity'),
  );
}
