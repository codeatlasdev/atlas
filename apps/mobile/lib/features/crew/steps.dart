/// Step phrases for crewmate tasks — port of apps/desktop/src/lib/crew/stepPhrase.ts
///
/// A step is `"<tool> <first string argument>"` as recorded by hostd
/// (crates/atlas-host/src/crew/runner.rs). These helpers turn raw step text
/// into short, readable phrases for the chat UI.
library;

// ── helpers ──────────────────────────────────────────────────────────────────

/// Last path segment; omp's hashline edits name the target "[path#TAG]",
/// so the brackets and tag are stripped first.
String _base(String p) {
  // Strip a hashline anchor "[path#TAG]" or truncated "[path#TAG" (hostd keeps 80 chars).
  // Capture just the path part before the first '#' or ']'.
  String path = p;
  final anchorMatch = RegExp(r'^\[([^#\]]+)').firstMatch(p);
  if (anchorMatch != null) {
    path = anchorMatch[1]!;
  }
  final segments = path.split('/').where((s) => s.isNotEmpty).toList();
  return segments.isEmpty ? path : segments.last;
}

// Shell command → phrase table (first match wins).
final _kShell = <(RegExp, String)>[
  (
    RegExp(
      r'^(bun|npm|pnpm|yarn)\s+(run\s+)?test\b|^cargo\s+test\b|^pytest\b|^go\s+test\b|^flutter\s+test\b',
    ),
    'Running the tests',
  ),
  (
    RegExp(
      r'^(bun|npm|pnpm|yarn)\s+(install|add|i)\b|^cargo\s+(add|fetch)\b|^pip\s+install\b',
    ),
    'Installing dependencies',
  ),
  (
    RegExp(
      r'^(cargo\s+(build|check)|bun\s+run\s+build|npm\s+run\s+build|tsc\b|svelte-check\b)',
    ),
    'Building',
  ),
  (RegExp(r'^gh\b'), 'Working on GitHub'),
  (RegExp(r'^git\b'), 'Checking git'),
];

// ── stepPhrase ────────────────────────────────────────────────────────────────

/// Turns a raw step string (`"<tool> <first-arg>"`) into a short readable phrase.
String stepPhrase(String? step) {
  final s = (step ?? '').trim();
  if (s.isEmpty) return 'Working';

  final space = s.indexOf(' ');
  final tool = (space < 0 ? s : s.substring(0, space)).toLowerCase();
  final arg = space < 0 ? '' : s.substring(space + 1).trim();

  switch (tool) {
    case 'read':
      return arg.isNotEmpty ? 'Reading ${_base(arg)}' : 'Reading';
    case 'edit':
    case 'multiedit':
      return arg.isNotEmpty ? 'Editing ${_base(arg)}' : 'Editing';
    case 'write':
      return arg.isNotEmpty ? 'Writing ${_base(arg)}' : 'Writing';
    case 'grep':
      return arg.isNotEmpty ? 'Searching for "$arg"' : 'Searching';
    case 'glob':
      return 'Looking for files';
    case 'bash':
    case 'shell':
    case 'exec':
      // "cd apps/store && bun test": skip the folder change, use what follows.
      final cmd = arg.replaceFirst(RegExp(r'^cd\s+\S+\s*(&&|;)\s*'), '');
      for (final (re, phrase) in _kShell) {
        if (re.hasMatch(cmd)) return phrase;
      }
      final program = cmd.split(RegExp(r'\s+')).first;
      return program.isNotEmpty
          ? 'Running ${_base(program)}'
          : 'Running a command';
  }

  // Unknown tools (MCP and the like): last segment of the id, as words.
  final name = tool.split('__').last.replaceAll(RegExp(r'[_-]+'), ' ');
  return 'Using $name';
}

// ── workingLabel ──────────────────────────────────────────────────────────────

/// Short verbs shown while the agent is between tool calls.
const idleWords = [
  'Thinking',
  'Planning',
  'Looking around',
  'Working',
  'Figuring it out',
  'Almost there',
];

/// How long a real step stays on the working line before idle words take over.
const _kStepHoldMs = 4000;

/// How long each idle word stays on screen.
const _kIdleWordMs = 2600;

/// The working line: the latest step phrase while it is fresh, then idle words
/// that cycle deterministically from the clock (no flicker on re-render).
String workingLabel(String? step, int? stepAtMs, int nowMs) {
  if (step != null &&
      step.isNotEmpty &&
      stepAtMs != null &&
      nowMs - stepAtMs < _kStepHoldMs) {
    return stepPhrase(step);
  }
  return idleWords[(nowMs ~/ _kIdleWordMs) % idleWords.length];
}

// ── stepGroups ────────────────────────────────────────────────────────────────

/// One section of a task's work: consecutive steps under the same agent title.
class StepGroup<S> {
  const StepGroup({
    required this.title,
    required this.steps,
    required this.done,
  });

  /// The agent's title for this stretch of work; null when it never set one.
  final String? title;
  final List<S> steps;

  /// Finished: either a later group has started, or the task is no longer running.
  final bool done;

  StepGroup<S> copyWith({String? title, List<S>? steps, bool? done}) =>
      StepGroup(
        title: title ?? this.title,
        steps: steps ?? this.steps,
        done: done ?? this.done,
      );
}

/// Partition [steps] into groups by consecutive agent title.
///
/// [groupOf] extracts the optional group label from each step. Steps before
/// omp's first real title join that first titled group rather than standing
/// untitled. Steps that never got a title take [fallback] (the task's own
/// title). The last group is not done while [running] is true.
List<StepGroup<S>> stepGroups<S>(
  List<S> steps,
  bool running, {
  required String? Function(S) groupOf,
  String? fallback,
}) {
  final groups = <StepGroup<S>>[];

  for (final step in steps) {
    final g = groupOf(step);
    final last = groups.isEmpty ? null : groups.last;
    if (last != null && (last.title == g || g == null)) {
      groups[groups.length - 1] = last.copyWith(steps: [...last.steps, step]);
    } else if (last != null && last.title == null) {
      // First title arrives: absorb all earlier untitled steps.
      groups[groups.length - 1] = last.copyWith(
        title: g,
        steps: [...last.steps, step],
      );
    } else {
      groups.add(StepGroup(title: g, steps: [step], done: false));
    }
  }

  for (var i = 0; i < groups.length; i++) {
    final isDone = !running || i < groups.length - 1;
    final title = groups[i].title ?? fallback;
    groups[i] = groups[i].copyWith(done: isDone, title: title);
  }

  return groups;
}
