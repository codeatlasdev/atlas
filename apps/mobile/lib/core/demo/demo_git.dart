/// The demo computer's repository: a few changed files, a branch and a remote, answering hostd's
/// `git_*` commands (`crates/atlas-host/src/git.rs`) in their real shapes.
library;

class _File {
  _File(this.path, this.x, this.y, this.old, this.neu);
  final String path;

  /// Index column and worktree column, as in `git status --porcelain=v2`.
  String x;
  String y;
  final List<String> old;
  final List<String> neu;
}

class DemoGit {
  final _files = <_File>[
    _File(
      'src/checkout.ts',
      'M',
      '.',
      [
        'export function total(items: Item[]) {',
        '  return items.reduce((n, i) => n + i.price, 0);',
        '}',
      ],
      [
        'export function total(items: Item[]) {',
        '  const sum = items.reduce((n, i) => n + i.price, 0);',
        '  return applyDiscount(sum);',
        '}',
      ],
    ),
    _File(
      'README.md',
      '.',
      'M',
      ['# orbit-shop'],
      ['# orbit-shop', '', 'A demo store.'],
    ),
    _File('src/discount.ts', '?', '?', [], [
      'export function applyDiscount(n: number) {',
      '  return n > 100 ? n * 0.9 : n;',
      '}',
    ]),
  ];

  final _branch = 'feat/discounts';
  var _ahead = 0;
  var _pushed = false;
  var _commits = 0;

  Map<String, Object?> status() => {
    'branch': _branch,
    'upstream': _pushed ? 'origin/$_branch' : null,
    'ab': _pushed ? '+$_ahead -0' : null,
    'entries': [
      for (final f in _files)
        {'path': f.path, 'x': f.x, 'y': f.y, if (f.x != '?') 'conflict': false},
    ],
  };

  Map<String, Object?> _fileDiff(_File f, {required bool staged}) {
    final add = f.x == '?';
    final lines = <Map<String, Object?>>[];
    var o = 1, n = 1;
    if (add) {
      for (final l in f.neu) {
        lines.add({'kind': 'add', 'text': l, 'new_no': n++});
      }
    } else {
      for (final l in f.old) {
        if (f.neu.contains(l)) {
          lines.add({'kind': 'ctx', 'text': l, 'old_no': o++, 'new_no': n++});
        } else {
          lines.add({'kind': 'del', 'text': l, 'old_no': o++});
        }
      }
      for (final l in f.neu.where((l) => !f.old.contains(l))) {
        lines.add({'kind': 'add', 'text': l, 'new_no': n++});
      }
    }
    return {
      'path': f.path,
      'status': add ? 'untracked' : 'modified',
      'staged': staged,
      'binary': false,
      'hunks': [
        {
          'header': add
              ? '@@ -0,0 +1,${f.neu.length} @@'
              : '@@ -1,${f.old.length} +1,${f.neu.length} @@',
          'old_start': add ? 0 : 1,
          'old_lines': add ? 0 : f.old.length,
          'new_start': 1,
          'new_lines': f.neu.length,
          'lines': lines,
        },
      ],
    };
  }

  Map<String, Object?> diff(String? path, bool staged) => {
    'files': [
      for (final f in _files)
        if ((path == null || f.path == path) &&
            (staged ? (f.x != '.' && f.x != '?') : (f.y != '.' || f.x == '?')))
          _fileDiff(f, staged: staged),
    ],
    'truncated': false,
  };

  /// Null = done, else the error hostd would answer.
  String? stage(List<String> paths, {required bool on}) {
    for (final p in paths) {
      final f = _files.where((f) => f.path == p).firstOrNull;
      if (f == null) return 'path outside the project: $p';
      if (on) {
        f.x = f.x == '?' ? 'A' : 'M';
        f.y = '.';
      } else {
        f.x = f.x == 'A' ? '?' : '.';
        f.y = f.x == '?' ? '?' : 'M';
      }
    }
    return null;
  }

  Map<String, Object?> message() => {
    'message':
        'feat(checkout): apply a 10% discount above 100\n\n'
        'Orders over 100 get a discount at the total, so the cart does not need to know about it.',
    'agent': 'omp',
    'scope': 'staged',
    'withheld': 0,
  };

  ({Map<String, Object?>? data, String? error}) commit(String message) {
    if (message.trim().isEmpty) {
      return (data: null, error: 'empty commit message');
    }
    if (!_files.any((f) => f.x != '.' && f.x != '?')) {
      return (data: null, error: 'nothing to commit');
    }
    _files.removeWhere((f) => f.x != '.' && f.x != '?' && f.y == '.');
    for (final f in _files.where((f) => f.x != '.' && f.x != '?')) {
      f.x = '.';
    }
    _ahead++;
    _commits++;
    return (data: {'sha': 'a${_commits}b2c3d'}, error: null);
  }

  Map<String, Object?> push() {
    _pushed = true;
    _ahead = 0;
    return {'branch': _branch, 'output': 'To origin\n * [new branch] $_branch'};
  }

  Map<String, Object?> pr(String title) => {
    'url': 'https://github.com/demo/orbit-shop/pull/42',
    'number': 42,
  };
}
