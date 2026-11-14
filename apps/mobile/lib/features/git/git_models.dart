/// What hostd's `git_*` commands answer (`crates/atlas-host/src/git.rs`), as the Changes screen reads it.
library;

/// One path of `git status`: `x` is the index column, `y` the working tree (`?` = untracked).
class GitEntry {
  const GitEntry({required this.path, required this.x, required this.y});

  factory GitEntry.fromJson(Map<String, dynamic> j) => GitEntry(
    path: j['path'] as String? ?? '',
    x: j['x'] as String? ?? '.',
    y: j['y'] as String? ?? '.',
  );

  final String path;
  final String x;
  final String y;

  bool get untracked => x == '?';

  /// Something of this file is in the index (never for an untracked one).
  bool get hasStaged => !untracked && x != '.' && x != ' ' && x != '!';

  /// Something of this file is not staged yet (a worktree change or a new file).
  bool get hasUnstaged => untracked || (y != '.' && y != ' ');

  /// One letter for a row: `M`odified, `A`dded, `D`eleted, `R`enamed, `U` for new untracked.
  String badge({required bool staged}) {
    if (untracked) return 'U';
    final c = staged ? x : y;
    return c == '.' ? '' : c;
  }
}

class GitStatus {
  const GitStatus({
    required this.branch,
    this.upstream,
    this.ab,
    this.entries = const [],
  });

  factory GitStatus.fromJson(Map<String, dynamic> j) => GitStatus(
    branch: j['branch'] as String? ?? '',
    upstream: j['upstream'] as String?,
    ab: j['ab'] as String?,
    entries: [
      for (final e in (j['entries'] as List<dynamic>?) ?? const [])
        GitEntry.fromJson((e as Map).cast<String, dynamic>()),
    ],
  );

  final String branch;
  final String? upstream;

  /// `+ahead -behind` against the upstream, as git prints it.
  final String? ab;
  final List<GitEntry> entries;

  List<GitEntry> get staged => [
    for (final e in entries)
      if (e.hasStaged) e,
  ];
  List<GitEntry> get unstaged => [
    for (final e in entries)
      if (e.hasUnstaged) e,
  ];

  int get ahead {
    final m = RegExp(r'\+(\d+)').firstMatch(ab ?? '');
    return m == null ? 0 : int.parse(m.group(1)!);
  }

  /// Commits on the branch that the remote does not have: a branch with no upstream has never been pushed.
  bool get needsPush => upstream == null || ahead > 0;
}

class DiffLine {
  const DiffLine({
    required this.kind,
    required this.text,
    this.oldNo,
    this.newNo,
  });

  factory DiffLine.fromJson(Map<String, dynamic> j) => DiffLine(
    kind: j['kind'] as String? ?? 'ctx',
    text: j['text'] as String? ?? '',
    oldNo: j['old_no'] as int?,
    newNo: j['new_no'] as int?,
  );

  /// `ctx`, `add` or `del`.
  final String kind;
  final String text;
  final int? oldNo;
  final int? newNo;
}

class DiffHunk {
  const DiffHunk({required this.header, required this.lines});

  factory DiffHunk.fromJson(Map<String, dynamic> j) => DiffHunk(
    header: j['header'] as String? ?? '',
    lines: [
      for (final l in (j['lines'] as List<dynamic>?) ?? const [])
        DiffLine.fromJson((l as Map).cast<String, dynamic>()),
    ],
  );

  final String header;
  final List<DiffLine> lines;
}

class FileDiff {
  const FileDiff({
    required this.path,
    required this.status,
    required this.staged,
    required this.binary,
    this.oldPath,
    this.hunks = const [],
  });

  factory FileDiff.fromJson(Map<String, dynamic> j) => FileDiff(
    path: j['path'] as String? ?? '',
    oldPath: j['old_path'] as String?,
    status: j['status'] as String? ?? 'modified',
    staged: j['staged'] as bool? ?? false,
    binary: j['binary'] as bool? ?? false,
    hunks: [
      for (final h in (j['hunks'] as List<dynamic>?) ?? const [])
        DiffHunk.fromJson((h as Map).cast<String, dynamic>()),
    ],
  );

  final String path;
  final String? oldPath;

  /// `modified`, `added`, `deleted`, `renamed` or `untracked`.
  final String status;
  final bool staged;
  final bool binary;
  final List<DiffHunk> hunks;

  int get added => _count('add');
  int get removed => _count('del');
  int _count(String kind) =>
      hunks.fold(0, (n, h) => n + h.lines.where((l) => l.kind == kind).length);

  /// Unified-diff text (`@@` headers, `+`/`-`/space lines): what `AtlasCodeBlock` tints for `diff`.
  String asPatch() => [
    for (final h in hunks) ...[
      h.header,
      for (final l in h.lines)
        '${switch (l.kind) {
          'add' => '+',
          'del' => '-',
          _ => ' ',
        }}${l.text}',
    ],
  ].join('\n');
}

class GitDiff {
  const GitDiff({required this.files, required this.truncated});

  factory GitDiff.fromJson(Map<String, dynamic> j) => GitDiff(
    files: [
      for (final f in (j['files'] as List<dynamic>?) ?? const [])
        FileDiff.fromJson((f as Map).cast<String, dynamic>()),
    ],
    truncated: j['truncated'] as bool? ?? false,
  );

  final List<FileDiff> files;

  /// hostd cut the patch at its size cap: the last file may be incomplete.
  final bool truncated;
}
