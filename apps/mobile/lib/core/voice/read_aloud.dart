/// "Read replies aloud": the per-phone switch, and the one rule for which finished tasks get spoken.
///
/// The switch lives on this phone only (off by default). Turning it on in a crewmate's chat also tells that
/// crewmate to write a listening summary (`crewmate_update {spoken_replies: true}`), see
/// `features/crew/chat/read_aloud_toggle.dart`; what is spoken is `task.spoken`, which hostd computes.
library;

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:shared_preferences/shared_preferences.dart';

import 'package:atlas_mobile/core/protocol/crew_task.dart';

/// Where the switch is kept. Tests use a fake.
abstract interface class ReadAloudStore {
  Future<bool> load();
  Future<void> save(bool on);
}

class PrefsReadAloudStore implements ReadAloudStore {
  static const _key = 'atlas_mobile.read_replies_aloud';

  /// Lazy: `SharedPreferencesAsync()` throws `StateError` when the platform is not set up (tests).
  /// The getter is only called on a real device; tests override [readAloudStoreProvider] with a fake.
  SharedPreferencesAsync get _prefs => SharedPreferencesAsync();

  @override
  Future<bool> load() async {
    try {
      return await _prefs.getBool(_key) ?? false;
    } on StateError {
      return false; // SharedPreferencesAsyncPlatform not set up (tests without a mock)
    }
  }

  @override
  Future<void> save(bool on) async {
    try {
      await _prefs.setBool(_key, on);
    } on StateError {
      // no-op in test environments without a platform mock
    }
  }
}

final readAloudStoreProvider = Provider<ReadAloudStore>(
  (_) => PrefsReadAloudStore(),
);

/// Whether replies are read aloud on this phone. Off until the stored value is read, so a slow read never
/// speaks by accident.
final readAloudProvider = NotifierProvider<ReadAloud, bool>(ReadAloud.new);

class ReadAloud extends Notifier<bool> {
  bool _touched = false;

  @override
  bool build() {
    _load();
    return false;
  }

  Future<void> _load() async {
    final on = await ref.read(readAloudStoreProvider).load();
    if (!_touched && ref.mounted) state = on;
  }

  Future<void> set(bool on) async {
    _touched = true;
    state = on;
    await ref.read(readAloudStoreProvider).save(on);
  }
}

/// The first thread it sees is history and says nothing (the screen only feeds it a thread that is not being read
/// again, so the cached copy shown while the chat re-reads is never the first). After that, a task that goes from
/// anything else to `done`, or turns up already `done`, is a reply that just arrived.
class SpokenReplies {
  final _states = <String, String>{};
  bool _started = false;

  /// The text to say for [tasks] (any order; spoken oldest first), empty when there is nothing new. A task with
  /// an empty `spoken` says nothing, and is not retried.
  String observe(List<CrewmateTask> tasks) {
    final fresh = <CrewmateTask>[];
    for (final t in tasks) {
      final before = _states[t.id];
      _states[t.id] = t.state;
      if (_started &&
          t.state == 'done' &&
          before != 'done' &&
          t.spoken.trim().isNotEmpty) {
        fresh.add(t);
      }
    }
    _started = true;
    fresh.sort((a, b) => a.ts.compareTo(b.ts));
    return fresh.map((t) => t.spoken.trim()).join(' ');
  }
}
