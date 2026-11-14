import 'dart:async';
import 'dart:convert';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/transport/transport.dart';

/// Strings real computers produce that break layouts: a hostname with no break
/// opportunity, a 64-char hash, a deep path, a long identifier, a line of prose.
const longHost = 'matheus-de-jesus-macbook-pro-16-inch-m4-pro-office.local';
const longHash =
    'f3a91c0be7d4428a9c5e1b6f7d02a8c4e9b1d3f5a7c9e1b3d5f7a9c1e3b5d7f9';
const longPath =
    '/Users/matheusdejesus/Projetos/codeatlasdev/atlas/apps/mobile/lib/features/agents/timeline_widgets.dart';
const longName =
    'orbit-shop-storefront-checkout-payments-ledger-reconciliation';
const longProse =
    'Refactor the connection notifier so that a reconnect after a long background '
    'period does not wait out the twenty second request deadline before it probes';

const stressDevice = MacDevice(nodeId: 'stress', name: longHost);

/// A computer that answers like `atlas hostd` but with [longName]-style data in every
/// field a screen prints, so a screen that only fits short strings overflows.
///
/// It wraps another [Transport] (the demo computer) and rewrites the replies that carry
/// names, paths and messages. Every other command passes through untouched.
class StressComputer implements Transport {
  StressComputer(this._inner) {
    _sub = _inner.incomingLines.listen(_onLine, onDone: _out.close);
  }

  final Transport _inner;
  final _out = StreamController<String>.broadcast();
  late final StreamSubscription<String> _sub;
  final _cmdById = <Object?, String>{};

  @override
  Stream<String> get incomingLines => _out.stream;

  @override
  Future<void> sendLine(String line) {
    final m = jsonDecode(line) as Map<String, dynamic>;
    _cmdById[m['id']] = m['cmd'] as String;
    return _inner.sendLine(line);
  }

  @override
  Future<void> close() {
    // The inner computer first, and without waiting: its timers must be cancelled before
    // the test body returns (a fake-async test never runs the event loop that would
    // finish an awaited close).
    final inner = _inner.close();
    unawaited(_sub.cancel());
    if (!_out.isClosed) unawaited(_out.close());
    return inner;
  }

  void _onLine(String line) {
    if (_out.isClosed) return;
    final m = jsonDecode(line) as Map<String, dynamic>;
    final cmd = _cmdById.remove(m['id']);
    if (m['ok'] == true && m['data'] != null) {
      m['data'] = _rewrite(cmd, m['data']);
    } else if (m['ev'] == 'agent_status') {
      m['status'] = _status(m['status'] as Map<String, dynamic>);
    }
    _out.add(jsonEncode(m));
  }

  Object? _rewrite(String? cmd, Object? data) {
    if (data is! Map<String, dynamic>) return data;
    switch (cmd) {
      case 'project_list':
        data['projects'] = [
          for (final p in data['projects'] as List) _project(p as Map),
        ];
      case 'project_detail':
        _detail(data);
      case 'agent_status_all':
        data['panes'] = {
          for (final e in (data['panes'] as Map).entries)
            e.key: _status((e.value as Map).cast<String, dynamic>()),
        };
      case 'agent_history':
        data['title'] = longProse;
        data['items'] = [
          for (final i in data['items'] as List) _item((i as Map).cast()),
        ];
      case 'agent_tree':
        data['nodes'] = [
          for (final n in data['nodes'] as List) _node((n as Map).cast()),
        ];
      case 'project_logs':
        data['text'] =
            '[$longName:1] 12:01:02 [stdout] $longHash $longPath\n'
            '[api:2] 12:01:03 [stderr] ERROR $longProse\n'
            '$longHash';
    }
    return data;
  }

  Map<String, dynamic> _project(Map p) {
    final j = p.cast<String, dynamic>();
    j['name'] = '$longName-${j['name']}';
    j['root'] = longPath;
    j['services'] = [
      for (final s in (j['services'] as List).cast<Map>())
        {...s, 'name': 'service-$longName-${s['name']}'},
      // A dozen services, as a monorepo has.
      for (var i = 0; i < 12; i++)
        {
          'name': 'svc-$i',
          'state': i.isEven ? 'running' : 'failed',
          'port': 3000 + i,
        },
    ];
    j['agents'] = [
      for (final a in (j['agents'] as List).cast<Map>())
        {...a, 'status': _status((a['status'] as Map).cast<String, dynamic>())},
    ];
    return j;
  }

  void _detail(Map<String, dynamic> d) {
    d['name'] = longName;
    d['root'] = longPath;
    final st = d['status'] as Map<String, dynamic>;
    st['setup'] = {
      'state': 'running',
      'command': 'bun install --frozen $longPath',
    };
    st['services'] = [
      for (final s in (st['services'] as List).cast<Map>())
        {...s, 'name': 'service-$longName-${s['name']}'},
    ];
    final cfg = d['config'] as Map<String, dynamic>;
    cfg['services'] = [
      for (final s in (cfg['services'] as List).cast<Map>())
        {...s, 'name': 'service-$longName-${s['name']}'},
    ];
    cfg['actions'] = [
      {
        'key': 'migrate-all-the-databases',
        'label': longProse,
        'confirm': longProse,
      },
    ];
    d['last_lines'] = {
      for (final s in (st['services'] as List).cast<Map>())
        s['name']: '$longHash $longPath',
    };
    d['agents'] = [
      for (final a in (d['agents'] as List).cast<Map>())
        {...a, 'status': _status((a['status'] as Map).cast<String, dynamic>())},
    ];
  }

  Map<String, dynamic> _status(Map<String, dynamic> s) => {
    ...s,
    'title': longProse,
    'prompt': longProse,
    if (s['last_message'] != null) 'last_message': '$longProse $longHash',
    if (s['model'] != null)
      'model': 'anthropic/claude-sonnet-5-5-20260101-long',
  };

  Map<String, dynamic> _item(Map<String, dynamic> i) {
    switch (i['kind']) {
      case 'user':
      case 'assistant':
        i['text'] =
            '$longProse\n\n`$longHash`\n\n$longPath\n\n```\n$longPath $longHash\n```\n\n'
            '| name | value |\n|---|---|\n| $longName | $longHash |';
      case 'tool':
        i['intent'] = longProse;
        i['detail'] = '$longPath $longHash';
        i['output'] = '$longHash\n$longPath';
        if (i['agents'] != null) {
          i['agents'] = [
            for (final a in (i['agents'] as List).cast<Map>())
              {...a, 'name': longName, 'agent': longName, 'task': longProse},
          ];
        }
      case 'result':
        i['id'] = longName;
    }
    return i;
  }

  Map<String, dynamic> _node(Map<String, dynamic> n) {
    final isRoot = n['id'] == '';
    return {
      ...n,
      if (!isRoot) 'id': '${n['id']}-$longName',
      if (!isRoot && n['parent'] != '' && n['parent'] != null)
        'parent': '${n['parent']}-$longName',
      if (!isRoot) 'name': '${n['name']}-$longName',
      'task': longProse,
      'now': longProse,
      'model': 'anthropic/claude-sonnet-5-5-20260101-long',
      'report': '$longProse $longHash',
    };
  }
}
