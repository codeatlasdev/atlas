// Contract: a task from GitHub keeps its `origin`, and a GitHub automation round-trips every field
// hostd sends (authors, allow_auto, handle, assignee), from the file hostd's real serializer wrote.
// Incident guarded: the editors rebuilt the automation as {type, repo, events}, so saving an edit
// reset `authors` and `allow_auto` to their defaults on the server.
// Regenerate fixture: UPDATE_FIXTURES=1 cargo test -p atlas-host github_origin_fixture_is_real_payload
import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

import 'package:atlas_mobile/core/protocol/crew_models.dart';
import 'package:atlas_mobile/features/crew/automations.dart';

void main() {
  late Map<String, dynamic> fixture;

  setUpAll(() {
    fixture = jsonDecode(
      File('test/fixtures/github_origin.json').readAsStringSync(),
    ) as Map<String, dynamic>;
  });

  test('CrewmateTask parses the GitHub origin', () {
    final task = CrewmateTask.fromJson(
      (fixture['task'] as Map).cast<String, dynamic>(),
    );
    final o = task.origin;
    expect(o, isNotNull);
    expect(o!.repo, 'o/r');
    expect(o.kind, 'pr_review_comment');
    expect(o.number, 12);
    expect(o.commentId, 200);
    expect(o.branch, 'feat/parser');
    expect(o.replyCommentId, 9001);
  });

  test('a task without an origin has none', () {
    final raw = Map<String, dynamic>.from(fixture['task'] as Map)
      ..remove('origin');
    expect(CrewmateTask.fromJson(raw).origin, isNull);
  });

  test('a GitHub trigger writes back every field it was read with', () {
    final kind = (fixture['trigger'] as Map)['kind'] as Map<String, dynamic>;
    final parsed = TriggerKind.fromJson(kind);
    expect(parsed, isA<GithubTrigger>());
    expect(parsed.toJson(), kind);
  });

  test('the new events read as sentences, not as raw keys', () {
    String d(List<String> e) =>
        describeTrigger(GithubTrigger(repo: 'o/r', events: e));
    expect(d(['mention']), 'When you are mentioned in o/r');
    expect(d(['issue_assigned']), 'When an issue is assigned to you in o/r');
    expect(d(['pr_review_comment']), 'When a PR gets a review comment in o/r');
  });
}
