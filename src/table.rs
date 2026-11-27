use std::borrow::Cow;

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const GAP: usize = 2;

pub(crate) fn render(header: &[String], rows: &[Vec<String>], width: usize) -> String {
    if header.is_empty() {
        return String::new();
    }
    let widths = column_widths(header, rows, width.max(1));
    let mut out = String::new();
    push_row(&mut out, header, &widths);
    for row in rows {
        push_row(&mut out, row, &widths);
    }
    out
}

fn column_widths(header: &[String], rows: &[Vec<String>], width: usize) -> Vec<usize> {
    let mut natural = vec![0usize; header.len()];
    for (i, cell) in header.iter().enumerate() {
        natural[i] = display_width(&sanitize(cell));
    }
    for row in rows {
        for (i, cell) in row.iter().enumerate().take(natural.len()) {
            natural[i] = natural[i].max(display_width(&sanitize(cell)));
        }
    }
    fit_widths(&natural, width)
}

/// Shrink columns until they fit in `term`.
///
/// Extra width comes off the longest columns first, so one long cell does
/// not shove the others off the screen.
pub(crate) fn fit_widths(natural: &[usize], term: usize) -> Vec<usize> {
    let n = natural.len();
    if n == 0 {
        return Vec::new();
    }
    let mut widths = natural.to_vec();
    let gap_total = GAP.saturating_mul(n.saturating_sub(1));
    if term <= gap_total {
        return vec![1; n];
    }
    let budget = term - gap_total;
    let sum: usize = widths.iter().sum();
    if sum <= budget {
        return widths;
    }
    let mut excess = sum - budget;
    while excess > 0 {
        let max = widths.iter().copied().max().unwrap_or(0);
        if max <= 1 {
            break;
        }
        // On a tie, shrink from the right so the leading columns stay readable.
        let Some(slot) = widths.iter_mut().rev().find(|w| **w == max) else {
            break;
        };
        *slot -= 1;
        excess -= 1;
    }
    widths
}

fn push_row(out: &mut String, cells: &[String], widths: &[usize]) {
    let mut line = String::new();
    for (i, width) in widths.iter().copied().enumerate() {
        if i > 0 {
            line.push_str("  ");
        }
        let cell = cells.get(i).map(String::as_str).unwrap_or("");
        line.push_str(&fit_cell(cell, width));
    }
    while line.ends_with(' ') {
        line.pop();
    }
    out.push_str(&line);
    out.push('\n');
}

fn fit_cell(cell: &str, width: usize) -> String {
    let clean = sanitize(cell);
    let shown = truncate_to(clean.as_ref(), width);
    let mut out = shown;
    let dw = UnicodeWidthStr::width(out.as_str());
    if dw < width {
        out.push_str(&" ".repeat(width - dw));
    }
    out
}

fn truncate_to(s: &str, width: usize) -> String {
    if UnicodeWidthStr::width(s) <= width {
        return s.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let mut kept = String::new();
    let mut used = 0;
    let limit = width - 1;
    for ch in s.chars() {
        let cw = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + cw > limit {
            break;
        }
        kept.push(ch);
        used += cw;
    }
    kept.push('…');
    kept
}

fn display_width(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

/// Newlines in a quoted field would break the grid. Display only; the file is unchanged.
fn sanitize(s: &str) -> Cow<'_, str> {
    if !s.bytes().any(|b| b < b' ') {
        return Cow::Borrowed(s);
    }
    Cow::Owned(
        s.chars()
            .map(|c| if (c as u32) < 0x20 { ' ' } else { c })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cells(row: &[&str]) -> Vec<String> {
        row.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn pads_short_cells() {
        let header = cells(&["name", "age"]);
        let rows = vec![cells(&["ann", "3"]), cells(&["bob", "40"])];
        let out = render(&header, &rows, 80);
        assert_eq!(out, "name  age\nann   3\nbob   40\n");
    }

    #[test]
    fn truncates_the_longest_column() {
        let header = cells(&["name", "city"]);
        let rows = vec![cells(&["ann", "a very long city name"])];
        let out = render(&header, &rows, 16);
        assert_eq!(out, "name  city\nann   a very lo…\n");
        for line in out.lines() {
            assert!(UnicodeWidthStr::width(line) <= 16, "{line}");
        }
    }

    #[test]
    fn wide_characters_count_as_two() {
        let header = cells(&["名", "n"]);
        let rows = vec![cells(&["字", "1"])];
        let out = render(&header, &rows, 80);
        assert_eq!(out, "名  n\n字  1\n");
    }

    #[test]
    fn truncates_a_wide_character_without_splitting_it() {
        let s = truncate_to("字字", 2);
        assert_eq!(s, "…");
        assert_eq!(UnicodeWidthStr::width(s.as_str()), 1);
        let s = truncate_to("字字", 3);
        assert_eq!(s, "字…");
        assert_eq!(UnicodeWidthStr::width(s.as_str()), 3);
    }

    #[test]
    fn shrinks_the_widest_column_before_the_narrow_ones() {
        assert_eq!(fit_widths(&[20, 5, 5], 20), vec![6, 5, 5]);
        assert_eq!(fit_widths(&[10, 10, 10], 24), vec![7, 7, 6]);
        assert_eq!(fit_widths(&[3, 3], 80), vec![3, 3]);
    }

    #[test]
    fn replaces_control_characters() {
        let header = cells(&["note"]);
        let rows = vec![cells(&["a\nb\tc"])];
        assert_eq!(render(&header, &rows, 80), "note\na b c\n");
    }
}
