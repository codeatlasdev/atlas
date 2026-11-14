/// The GitHub device-flow login of a crew server (`crew_github_login_start` then polling
/// `crew_github_login_poll`), shared by the profile's Accounts card and the chat's
/// "Connect GitHub" action card. It owns the poll timer and the code being shown; the
/// screen decides what to do when the flow ends.
library;

import 'dart:async';

import 'package:atlas_mobile/core/protocol/crew_models.dart';
import 'package:atlas_mobile/core/protocol/multiplexer.dart';

/// How a polled login ended.
enum GithubPoll { pending, done, error }

class GithubDeviceLogin {
  GithubDeviceLogin({required this.onChanged});

  /// Called whenever [flow] or [polling] changes (the owner calls `setState`).
  final void Function() onChanged;

  CrewGithubLogin? flow;
  bool polling = false;
  Timer? _timer;

  /// Asks the server for a code and starts polling every 4 s through [poll].
  /// Returns the server's error text, or null when the flow started.
  Future<String?> start(Multiplexer mux, Future<void> Function() poll) async {
    final resp = await mux.call('crew_github_login_start', {});
    if (resp['ok'] != true) {
      return resp['error']?.toString() ?? 'Failed to start login';
    }
    flow = CrewGithubLogin.fromJson(
      (resp['data'] as Map?)?.cast<String, dynamic>() ?? {},
    );
    polling = true;
    _timer?.cancel();
    _timer = Timer.periodic(const Duration(seconds: 4), (_) => poll());
    onChanged();
    return null;
  }

  /// One poll. On `done` or `error` the flow is cleared; [data] is the server's payload
  /// (`login` on done, `error` on error).
  Future<(GithubPoll, Map<String, dynamic>)> pollOnce(Multiplexer mux) async {
    final resp = await mux.call('crew_github_login_poll', {});
    final data = (resp['data'] as Map?)?.cast<String, dynamic>() ?? {};
    final result = switch (data['state'] as String? ?? 'pending') {
      'done' => GithubPoll.done,
      'error' => GithubPoll.error,
      _ => GithubPoll.pending,
    };
    if (result != GithubPoll.pending) cancel();
    return (result, data);
  }

  /// Drops the code and stops polling.
  void cancel() {
    _timer?.cancel();
    _timer = null;
    flow = null;
    polling = false;
    onChanged();
  }

  void dispose() {
    _timer?.cancel();
  }
}
