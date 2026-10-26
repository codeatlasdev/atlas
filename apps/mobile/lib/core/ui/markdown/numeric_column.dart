/// A table column that can be drawn as bars: every body cell holds a number
/// (optionally with a unit: `40`, `8.4 GB`, `12%`, `1,234`).
class NumericColumn {
  const NumericColumn({
    required this.index,
    required this.values,
    required this.unit,
  });

  final int index;
  final List<double> values;

  /// The unit shared by the cells (`GB`, `%`), empty when there is none.
  final String unit;

  static final _cell = RegExp(
    r'^([-+]?(?:\d{1,3}(?:,\d{3})+|\d+)(?:\.\d+)?)\s*([A-Za-z%°µ/]*)$',
  );

  /// Parses one cell, or null when it is not a number.
  static (double, String)? parseCell(String raw) {
    final m = _cell.firstMatch(raw.trim());
    if (m == null) return null;
    final v = double.tryParse(m.group(1)!.replaceAll(',', ''));
    return v == null ? null : (v, m.group(2)!);
  }

  /// The first numeric column of [rows] (body cell texts). Column 0 is the label
  /// column and never counts. Null when there is none.
  static NumericColumn? find(List<List<String>> rows) {
    if (rows.isEmpty) return null;
    final cols = rows.map((r) => r.length).reduce((a, b) => a < b ? a : b);
    for (var c = 1; c < cols; c++) {
      final values = <double>[];
      var unit = '';
      var ok = true;
      for (final row in rows) {
        final p = parseCell(row[c]);
        if (p == null || (values.isNotEmpty && p.$2 != unit)) {
          ok = false;
          break;
        }
        if (values.isEmpty) unit = p.$2;
        values.add(p.$1);
      }
      if (ok) return NumericColumn(index: c, values: values, unit: unit);
    }
    return null;
  }
}
