/// The demo crew server: crewmates and their tasks, asks and action cards, the audit,
/// engine sign-ins and the pairing invite, in hostd's `crewmate_*` and `crew_*` shapes.
library;

import 'package:atlas_mobile/core/demo/demo_attachments.dart';
import 'package:atlas_mobile/core/demo/demo_bus.dart';
import 'package:atlas_mobile/core/demo/demo_crew.dart';

class DemoCrewServer {
  final DemoBus bus;

  DemoCrewServer(this.bus) {
    _crewmates.addAll(seedDemoCrew());
    // Add pending invite
    _pendingInvites.add({
      'url': 'atlas-mobile://pair?node=crew-server-1&code=CREWCODE&name=myserver&addr=192.168.1.10:4040',
      'name': 'myserver',
    });
  }

  final _crewmates = <String, DemoCrewmate>{};

  /// The crewmates of the demo crew server, by id.
  Map<String, DemoCrewmate> get crewmates => _crewmates;

  late final attachments = DemoAttachments(_crewmates);
  late final _ops = DemoCrewOps(_crewmates);
  var _paused = false;

  /// Engine sign-ins of the demo crew server: codex needs one, until the demo terminal "signs it in".
  final _engineState = <String, String>{
    'claude': 'signed_in',
    'codex': 'needs_signin',
    'omp': 'signed_in',
    'opencode': 'not_installed',
  };

  final _pendingInvites = <Map<String, String>>[];

  String? _createCrewmateTask(
    String? crewmateId,
    String prompt,
    String source,
    List<dynamic> images, {
    bool ui = false,
  }) {
    final crewmate = _crewmates[crewmateId];
    if (crewmate == null) return 'crewmate not found';
    final taskId = 'task-${bus.seq + 1}';
    final taken = attachments.takeImages(taskId, images);
    if (taken.error != null) return taken.error;
    bus.seq++;
    final ts = demoNowSeconds();
    final task = <String, dynamic>{
      'id': taskId,
      'ts': ts,
      'source': source,
      'prompt': prompt,
      'state': 'queued',
      'result': null,
      'error': null,
      'started_at': null,
      'finished_at': null,
      'steps': <Object?>[],
      if (taken.records.isNotEmpty) 'attachments': taken.records,
      if (ui) 'ui': true,
    };
    crewmate.tasks.insert(0, task);
    crewmate.queued++;
    // Simulate task running then done.
    bus.later(const Duration(milliseconds: 500), () {
      task['state'] = 'running';
      task['started_at'] = demoNowSeconds();
      crewmate.state = 'working';
      crewmate.queued = (crewmate.queued - 1).clamp(0, 999);
      _pushCrewmateTask(crewmate, task);
    });
    bus.later(const Duration(milliseconds: 2800), () {
      if (ui) return _parkUnproven(crewmate, task);
      task['state'] = 'done';
      task['finished_at'] = demoNowSeconds();
      task['result'] =
          'This is a demo. On your real crew server, ${crewmate.agent} would run and return the result here.';
      crewmate.state = 'idle';
      crewmate.lastEntry = {
        'text': task['result'],
        'state': 'done',
        'ts': demoNowSeconds(),
      };
      _pushCrewmateTask(crewmate, task);
    });
    return null;
  }

  /// A UI task finishes with one capture of its own: not the two it needs, so it parks on `prove_ui` like
  /// hostd's gate does (`runner/gate.rs`), with the capture attached as the agent's image.
  void _parkUnproven(DemoCrewmate crewmate, Map<String, dynamic> task) {
    final id = task['id'] as String;
    final name = 'chromium-390-light.png';
    attachments.images['$id/$name'] = (mime: 'image/png', bytes: demoCapture);
    final records = [...?(task['attachments'] as List?)];
    records.add({
      'name': name,
      'mime': 'image/png',
      'bytes_len': demoCapture.length,
      'sha256': DemoAttachments.digest(demoCapture),
      'path': '.atlas/attachments/$id/$name',
      'source': 'agent',
    });
    task['attachments'] = records;
    task['state'] = 'waiting_action';
    task['result'] = 'Restyled the header.';
    task['action'] = {
      'kind': 'prove_ui',
      'reason': 'Could not prove it: this is a UI task and the agent attached 1 of 2 captures. It must look at the result (the capture tool: before and after, top and end of the page) before it is done.',
    };
    crewmate.state = 'idle';
    crewmate.waiting = task;
    _pushCrewmateTask(crewmate, task);
  }

  /// `crewmate_answer`: records the answer on the task whose open `ask` has that id, with hostd's own
  /// messages for what it refuses (neither key nor text; an ask that is not there or already answered).
  String? _answerAsk(Map<String, dynamic> args) {
    final key = args['key'] as String?;
    final text = args['text'] as String?;
    if (key == null && text == null) return 'key or text required';
    final id = args['id'] as String?;
    final askId = args['ask_id'] as String?;
    for (final task
        in _crewmates[id]?.tasks ?? const <Map<String, Object?>>[]) {
      final ask = task['ask'];
      if (ask is Map && ask['id'] == askId && ask['answered'] == null) {
        ask['answered'] = key != null
            ? {'kind': 'key', 'value': key}
            : {'kind': 'text', 'value': text};
        return null;
      }
    }
    return 'ask $askId not found for crewmate $id';
  }

  /// `crewmate_action_answer` for the cards the demo raises: `override` accepts the work, `retry` runs it
  /// again (with the owner's marked screenshot stored on the task); images go with a retry only, checked
  /// with the same rules as a task's. Anything else is refused, and the card keeps waiting.
  String? _answerAction(Map<String, dynamic> args) {
    final crewmate = _crewmates[args['id'] as String?];
    final task = crewmate?.tasks
        .where((t) => t['id'] == args['task'] && t['state'] == 'waiting_action')
        .firstOrNull;
    if (crewmate == null || task == null) {
      return 'no waiting-action task ${args['task']} for crewmate ${args['id']}';
    }
    final value = args['value'] as String? ?? '';
    final kind = (task['action'] as Map?)?['kind'];
    if (kind != 'prove_ui' && kind != 'refused') return null;
    if (value != 'override' && value != 'retry') {
      return 'this card is answered with "override" or "retry"';
    }
    final sent = (args['attachments'] as List?) ?? const [];
    if (sent.isNotEmpty && value != 'retry') {
      return 'images go with a retry, not with an override';
    }
    final existing = (task['attachments'] as List?) ?? const [];
    final taken = attachments.takeImages(
      task['id'] as String,
      sent,
      existing: existing,
    );
    if (taken.error != null) return taken.error;
    task['attachments'] = [...existing, ...taken.records];
    task['action'] = null;
    crewmate.waiting = null;
    if (value == 'override') {
      task['state'] = 'done';
      task['finished_at'] = demoNowSeconds();
      _pushCrewmateTask(crewmate, task);
    } else {
      // Run again: the demo agent takes its second capture and the work is proven.
      task['state'] = 'running';
      _pushCrewmateTask(crewmate, task);
      bus.later(const Duration(milliseconds: 1500), () {
        task['state'] = 'done';
        task['finished_at'] = demoNowSeconds();
        _pushCrewmateTask(crewmate, task);
      });
    }
    return null;
  }

  void _pushCrewmateTask(DemoCrewmate crewmate, Map<String, dynamic> task) {
    bus.emit({
      'ev': 'crewmate_task',
      'crewmate_id': crewmate.id,
      'task': {
        'id': task['id'],
        'state': task['state'],
        'result': task['result'],
        'error': task['error'],
      },
    });
  }

  /// `agent_approve` on a crewmate pane (`crew:<crewmateId>:<taskId>`).
  String? approve(String pane, String? req) {
    final parts = pane.split(':');
    if (parts.length < 2) return 'invalid crewmate pane';
    final crewmateId = parts[1];
    final crewmate = _crewmates[crewmateId];
    if (crewmate == null ||
        req == null ||
        crewmate.pendingApproval?['req'] != req) {
      return 'no such pending request';
    }
    crewmate.pendingApproval = null;
    crewmate.state = 'idle';
    bus.emit({'ev': 'agent_approval_done', 'req': req, 'pane_key': pane});
    return null;
  }

  /// The reply to `cmd`, or null when it is not one of this part's commands.
  DemoReply? handle(String cmd, Map<String, dynamic> args) {
    Object? data;
    String? error;
    switch (cmd) {
      case 'crewmate_list':
        data = {
          'host_name': 'demo-mac',
          'crewmates': [for (final b in _crewmates.values) b.toListEntry()],
          'paused': _paused,
        };
      case 'crew_pause' || 'crew_resume':
        _paused = cmd == 'crew_pause';
        _ops.record(null, _paused ? 'paused' : 'resumed', {});
        bus.emit({'ev': 'crew_paused', 'paused': _paused});
        data = <String, Object?>{};
      case 'crewmate_tasks':
        final crewmateId = args['id'] as String?;
        final crewmate = _crewmates[crewmateId];
        data = {'tasks': crewmate?.tasks ?? const []};
      case 'crewmate_task':
        error = _createCrewmateTask(
          args['id'] as String?,
          args['prompt'] as String? ?? '',
          args['source'] as String? ?? 'mobile',
          (args['attachments'] as List?) ?? const [],
          ui: args['ui'] as bool? ?? false,
        );
        data = error == null
            ? {
                'task': {'id': 'task-${bus.seq}', 'state': 'queued'},
              }
            : null;
      case 'crewmate_answer':
        error = _answerAsk(args);
        data = error == null ? <String, Object?>{} : null;
      case 'crewmate_action_answer':
        error = _answerAction(args);
        data = error == null ? <String, Object?>{} : null;
      case 'crewmate_attachment_put':
        error = attachments.putPiece(args);
        data = error == null ? <String, Object?>{'staged': 0} : null;
      case 'crewmate_attachment':
        final piece = attachments.attachmentPiece(args);
        error = piece.$2;
        data = piece.$1;
      case 'crewmate_memory':
        final crewmate = _crewmates[args['id'] as String?];
        data = {'text': crewmate?.memory ?? '# Memory\nNothing saved yet.'};
      case 'crewmate_memory_set':
        final crewmate = _crewmates[args['id'] as String?];
        crewmate?.memory = args['text'] as String? ?? '';
        data = <String, Object?>{};
      case 'crewmate_routine_set':
        final crewmate = _crewmates[args['id'] as String?];
        final routines = [
          for (final r in (args['routines'] as List?) ?? const [])
            (r as Map).cast<String, dynamic>(),
        ];
        // hostd validates the cron before saving; so does the demo, with the same message.
        final bad = routines.where(
          (r) => !demoCronOk(r['cron'] as String? ?? ''),
        );
        if (crewmate == null) {
          error = 'crewmate ${args['id']} not found';
        } else if (bad.isNotEmpty) {
          error = "invalid cron '${bad.first['cron']}'";
        } else {
          _ops.record(crewmate.id, 'config_changed', {
            'field': 'routines',
            'from': crewmate.routines,
            'to': routines,
          });
          crewmate.routines = routines;
        }
        data = error == null ? <String, Object?>{} : null;
      case 'crewmate_audit' || 'crew_audit':
        data = _ops.audit(
          cmd == 'crew_audit' ? null : args['id'] as String?,
          since: (args['since'] as num?)?.toInt(),
          limit: (args['limit'] as num?)?.toInt() ?? 100,
          kind: args['kind'] as String?,
        );
      case 'crewmate_usage':
        data = _ops.usage(
          args['id'] as String?,
          since: (args['since'] as num?)?.toInt() ?? 0,
        );
      case 'engine_limits':
        data = {
          'message': 'no machine-readable quota or rate-limit information is exposed by any supported engine CLI',
          'limits': <String, Object?>{},
        };
      case 'crew_settings_get':
        data = {
          'short_replies': _ops.shortReplies,
          'short_replies_model': _ops.shortRepliesModel,
        };
      case 'crew_engine_status':
        data = {
          'engines': [
            for (final e in _engineState.entries)
              {'engine': e.key, 'state': e.value},
          ],
        };
      case 'crew_engine_login':
        final engine = args['engine'] as String? ?? '';
        if (!_engineState.containsKey(engine)) {
          error = 'unknown engine "$engine"';
        } else {
          // The demo terminal finishes the sign-in at once (the terminal itself is the fake one).
          _engineState[engine] = 'signed_in';
          data = {'engine': engine, 'pty': 'pty-signin-$engine'};
        }
      case 'crew_settings_set':
        _ops.shortReplies = args['short_replies'] as bool? ?? true;
        _ops.shortRepliesModel = args['short_replies_model'] as String?;
        data = <String, Object?>{};
      case 'crewmate_create':
        final newId = 'crewmate-${_crewmates.length + 1}';
        _crewmates[newId] = DemoCrewmate(
          newId,
          args['name'] as String? ?? 'Crewmate',
          args['agent'] as String? ?? 'claude',
          args['role'] as String? ?? '',
        );
        data = _crewmates[newId]!.toListEntry();
      case 'crewmate_delete':
        _crewmates.remove(args['id'] as String?);
        data = <String, Object?>{};
      case 'crewmate_cancel':
        data = <String, Object?>{};
      case 'mobile_invites':
        data = {'invites': _pendingInvites};
      case 'mobile_invite_dismiss':
        final url = args['url'] as String?;
        _pendingInvites.removeWhere((i) => i['url'] == url);
        data = <String, Object?>{};
      case 'crew_github_status':
        data = {'installed': true, 'login': null};
      case 'crew_github_login_start':
        data = {
          'user_code': 'DEMO-1234',
          'verification_uri': 'https://github.com/login/device',
        };
      case 'crew_github_login_poll':
        data = {'state': 'pending'};
      case 'crew_github_logout':
        data = <String, Object?>{};
      default:
        return null;
    }
    return (data: data, error: error);
  }
}
