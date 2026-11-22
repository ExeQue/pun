use pun_lint::problem::{Edit, Fix, LineCol, LineIndex, Safety, Span, apply_fixes};

fn lc(line: usize, column: usize) -> LineCol {
    LineCol { line, column }
}

fn fix(edits: &[(usize, usize, &str)]) -> Fix {
    Fix {
        safety: Safety::Safe,
        edits: edits
            .iter()
            .map(|&(s, e, t)| Edit::new(Span::new(s, e), t))
            .collect(),
    }
}

#[test]
fn line_col_on_lf_lines() {
    let index = LineIndex::new("a\nbc\n");
    assert_eq!(index.line_count(), 3);
    assert_eq!(index.line_col(0), lc(1, 1));
    assert_eq!(index.line_col(1), lc(1, 2));
    assert_eq!(index.line_col(2), lc(2, 1));
    assert_eq!(index.line_col(3), lc(2, 2));
    assert_eq!(index.line_col(5), lc(3, 1));
    assert_eq!(index.line_col(99), lc(3, 1), "clamped to the end");
    assert_eq!(index.line_range(0), None);
    assert_eq!(index.line_range(1), Some(0..1));
    assert_eq!(index.line_range(2), Some(2..4));
    assert_eq!(index.line_range(3), Some(5..5));
    assert_eq!(index.line_range(4), None);
}

#[test]
fn line_col_on_crlf_and_lone_cr() {
    let index = LineIndex::new("a\r\nb");
    assert_eq!(index.line_count(), 2);
    assert_eq!(index.line_col(1), lc(1, 2));
    assert_eq!(index.line_col(2), lc(1, 3));
    assert_eq!(index.line_col(3), lc(2, 1));
    assert_eq!(index.line_range(1), Some(0..1));
    assert_eq!(index.line_range(2), Some(3..4));

    let index = LineIndex::new("a\rb");
    assert_eq!(index.line_col(2), lc(2, 1));
    assert_eq!(index.line_range(1), Some(0..1));

    let index = LineIndex::new("\r\r\n\n");
    assert_eq!(index.line_count(), 4);
    assert_eq!(index.line_range(1), Some(0..0));
    assert_eq!(index.line_range(2), Some(1..1));
    assert_eq!(index.line_range(3), Some(3..3));
    assert_eq!(index.line_range(4), Some(4..4));
    assert_eq!(index.line_col(4), lc(4, 1));
}

#[test]
fn columns_count_bytes_of_multibyte_utf8() {
    let index = LineIndex::new("é = 1;\n  $ü");
    assert_eq!(index.line_col(3), lc(1, 4));
    assert_eq!(index.line_col(10), lc(2, 3));
    assert_eq!(index.line_col(11), lc(2, 4));
    assert_eq!(index.line_col(13), lc(2, 6));
    assert_eq!(index.line_range(2), Some(8..13));
}

#[test]
fn empty_source_has_one_empty_line() {
    let index = LineIndex::new("");
    assert_eq!(index.line_count(), 1);
    assert_eq!(index.line_col(0), lc(1, 1));
    assert_eq!(index.line_range(1), Some(0..0));
}

#[test]
fn edits_at_start_and_end() {
    let a = fix(&[(0, 0, "// x\n")]);
    let b = fix(&[(13, 13, "\n")]);
    let out = apply_fixes("<?php echo 1;", &[&a, &b]);
    assert_eq!(out.source, "// x\n<?php echo 1;\n");
    assert_eq!(out.applied, vec![0, 1]);
    assert!(out.deferred.is_empty() && out.invalid.is_empty());

    let all = fix(&[(0, 3, "xyz")]);
    assert_eq!(apply_fixes("abc", &[&all]).source, "xyz");
}

#[test]
fn edits_on_multibyte_and_crlf() {
    let ok = fix(&[(6, 8, "e")]);
    assert_eq!(apply_fixes("$a = 'é';", &[&ok]).source, "$a = 'e';");
    let split = fix(&[(6, 7, "e")]);
    let out = apply_fixes("$a = 'é';", &[&split]);
    assert_eq!(out.source, "$a = 'é';");
    assert_eq!(out.invalid, vec![0]);

    let eol = fix(&[(1, 3, "\n")]);
    assert_eq!(apply_fixes("a\r\nb", &[&eol]).source, "a\nb");
}

#[test]
fn overlapping_fixes_are_deferred_and_adjacent_ones_apply() {
    let x = fix(&[(1, 4, "X")]);
    let y = fix(&[(3, 5, "Y")]);
    let z = fix(&[(4, 6, "Z")]);
    let out = apply_fixes("abcdef", &[&x, &y, &z]);
    assert_eq!(out.source, "aXZ");
    assert_eq!(out.applied, vec![0, 2]);
    assert_eq!(out.deferred, vec![1]);

    let out = apply_fixes("abcdef", &[&y, &x]);
    assert_eq!(out.source, "abcYf");
    assert_eq!(out.deferred, vec![1]);
}

#[test]
fn insertions_touching_another_edit_are_deferred() {
    let r = fix(&[(1, 3, "X")]);
    let at_end = fix(&[(3, 3, "!")]);
    let at_start = fix(&[(1, 1, "!")]);
    let inside = fix(&[(2, 2, "!")]);
    let out = apply_fixes("abcd", &[&r, &at_end, &at_start, &inside]);
    assert_eq!(out.source, "aXd");
    assert_eq!(out.deferred, vec![1, 2, 3]);

    let out = apply_fixes("abcd", &[&at_start, &r]);
    assert_eq!(out.source, "a!bcd");
    assert_eq!(out.deferred, vec![1]);

    let a = fix(&[(0, 0, "a")]);
    let b = fix(&[(0, 0, "b")]);
    let out = apply_fixes("x", &[&a, &b]);
    assert_eq!(out.source, "ax");
    assert_eq!(out.deferred, vec![1]);
}

#[test]
fn a_fix_applies_whole_or_not_at_all() {
    let brackets = fix(&[(2, 2, "["), (3, 3, "]")]);
    let rename = fix(&[(0, 1, "g"), (2, 3, "y")]);
    let out = apply_fixes("f(x)", &[&brackets, &rename]);
    assert_eq!(out.source, "f([x])");
    assert_eq!(out.applied, vec![0]);
    assert_eq!(out.deferred, vec![1]);

    let unordered = fix(&[(3, 4, "B"), (0, 1, "A")]);
    assert_eq!(apply_fixes("abcd", &[&unordered]).source, "AbcB");
}

#[test]
fn invalid_fixes_never_apply() {
    let reversed = fix(&[(2, 1, "")]);
    let past_end = fix(&[(2, 5, "")]);
    let self_overlap = fix(&[(0, 2, "A"), (1, 3, "B")]);
    let ok = fix(&[(0, 1, "Z")]);
    let out = apply_fixes("abc", &[&reversed, &past_end, &self_overlap, &ok]);
    assert_eq!(out.source, "Zbc");
    assert_eq!(out.applied, vec![3]);
    assert_eq!(out.invalid, vec![0, 1, 2]);
    assert_eq!(out.deferred, [] as [usize; 0]);
}
