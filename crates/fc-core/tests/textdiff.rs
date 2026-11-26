use std::time::{Duration, Instant};

use fc_core::textdiff::codec::{self, DEFAULT_NEWLINE};
use fc_core::textdiff::*;

const EXACT: TextDiffOptions = TextDiffOptions {
    ignore_whitespace: false,
    ignore_case: false,
};

fn diff(left: &str, right: &str, o: TextDiffOptions) -> Vec<DiffBlock> {
    compare_lines(&split_lines(left), &split_lines(right), o)
}

fn blk(ls: usize, lc: usize, rs: usize, rc: usize, kind: DiffBlockKind) -> DiffBlock {
    DiffBlock::new(ls, lc, rs, rc, kind)
}

// ---- LineDiffer ----

#[test]
fn split_lines_like_an_editor() {
    assert_eq!(vec!["a", ""], split_lines("a\n"));
    assert_eq!(vec![""], split_lines(""));
    assert_eq!(vec!["a", "b", "c", "d"], split_lines("a\r\nb\nc\rd"));
    assert_eq!(vec!["", ""], split_lines("\r\n"));
}

#[test]
fn identical_is_one_equal_block() {
    assert_eq!(
        vec![blk(0, 3, 0, 3, DiffBlockKind::Equal)],
        diff("a\nb\nc", "a\nb\nc", EXACT)
    );
}

#[test]
fn inserted_line_is_right_only() {
    let blocks = diff("a\nc", "a\nb\nc", EXACT);
    assert!(blocks.contains(&blk(1, 0, 1, 1, DiffBlockKind::RightOnly)));
    assert_eq!(1, blocks.iter().filter(|b| b.is_difference()).count());
}

#[test]
fn deleted_line_is_left_only() {
    assert!(diff("a\nb\nc", "a\nc", EXACT).contains(&blk(1, 1, 1, 0, DiffBlockKind::LeftOnly)));
}

#[test]
fn changed_lines_are_changed() {
    assert!(diff("a\nb\nc\nd", "a\nX\nY\nd", EXACT).contains(&blk(
        1,
        2,
        1,
        2,
        DiffBlockKind::Changed
    )));
}

#[test]
fn empty_versus_text() {
    assert_eq!(
        vec![blk(0, 1, 0, 2, DiffBlockKind::Changed)],
        diff("", "x\ny", EXACT)
    );
}

#[test]
fn missing_trailing_newline_is_a_difference() {
    assert!(
        diff("a\nb\n", "a\nb", EXACT)
            .iter()
            .any(|b| b.is_difference())
    );
}

#[test]
fn whitespace_only_change_is_unimportant_when_ignored() {
    assert!(
        diff("a\n  b  c\nd", "a\nb c\nd", EXACT)
            .iter()
            .any(|b| b.kind == DiffBlockKind::Changed)
    );
    let o = TextDiffOptions {
        ignore_whitespace: true,
        ..EXACT
    };
    let blocks = diff("a\n  b  c\nd", "a\nb c\nd", o);
    assert!(blocks.contains(&blk(1, 1, 1, 1, DiffBlockKind::Unimportant)));
    assert!(!blocks.iter().any(|b| b.is_difference()));
}

#[test]
fn case_only_change_is_unimportant_when_ignored() {
    let o = TextDiffOptions {
        ignore_case: true,
        ..EXACT
    };
    assert_eq!(
        vec![blk(0, 1, 0, 1, DiffBlockKind::Unimportant)],
        diff("Hello", "hello", o)
    );
    assert_eq!(
        vec![blk(0, 1, 0, 1, DiffBlockKind::Unimportant)],
        diff("Grüße", "GRÜßE", o)
    );
}

#[test]
fn blocks_cover_both_sides_contiguously() {
    let blocks = diff("1\n2\n3\n4\n5\n6", "0\n2\n3\nx\n5\n7\n8", EXACT);
    let (mut l, mut r) = (0, 0);
    for b in &blocks {
        assert_eq!(l, b.left_start);
        assert_eq!(r, b.right_start);
        l += b.left_count;
        r += b.right_count;
    }
    assert_eq!((6, 7), (l, r));
}

/// Small deterministic generator (xorshift) for the property test.
struct Rng(u64);

impl Rng {
    fn next(&mut self, max: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % max as u64) as usize
    }
}

#[test]
fn random_edits_copying_every_block_to_the_left_reproduces_the_right() {
    let mut rnd = Rng(1234);
    let letter = |rnd: &mut Rng, n: usize| ((b'a' + rnd.next(n) as u8) as char).to_string();
    for _round in 0..200 {
        let len = rnd.next(30);
        let left: Vec<String> = (0..len).map(|_| letter(&mut rnd, 6)).collect();
        let mut right = left.clone();
        for _ in 0..rnd.next(8) {
            let op = rnd.next(3);
            if op == 0 || right.is_empty() {
                let at = rnd.next(right.len() + 1);
                right.insert(at, letter(&mut rnd, 8));
            } else if op == 1 {
                let at = rnd.next(right.len());
                right.remove(at);
            } else {
                let at = rnd.next(right.len());
                right[at] = letter(&mut rnd, 8);
            }
        }

        let left_text = left.join("\n");
        let right_text = right.join("\n");
        let blocks = compare_lines(&split_lines(&left_text), &split_lines(&right_text), EXACT);

        // Apply from the last block to the first so earlier line numbers stay valid.
        let mut text = left_text.clone();
        let right_source = StringLineSource::new(&right_text);
        for b in blocks.iter().rev().filter(|b| b.is_difference()) {
            let edit =
                compute_block_copy(&right_source, &StringLineSource::new(&text), b, false, "\n");
            text = edit.apply(&text);
        }
        assert_eq!(right_text, text, "left {left:?} right {right:?}");
    }
}

#[test]
fn large_dissimilar_files_finish_quickly() {
    let left: Vec<String> = (0..20000).map(|n| format!("L{n}")).collect();
    let right: Vec<String> = (0..20000)
        .map(|n| {
            if n % 3 == 0 {
                format!("L{n}")
            } else {
                format!("R{n}")
            }
        })
        .collect();
    let start = Instant::now();
    let blocks = compare_lines(&left, &right, EXACT);
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "{:?}",
        start.elapsed()
    );
    assert_eq!(20000, blocks.iter().map(|b| b.left_count).sum::<usize>());
}

#[test]
fn large_similar_files_with_many_changes() {
    // Every line matches somewhere, so the pre-filter can't help: exercises the middle snake and its fallback.
    let left: Vec<String> = (0..20000).map(|n| format!("{}", n % 50)).collect();
    let right: Vec<String> = (0..20000).map(|n| format!("{}", (n * 7) % 50)).collect();
    let start = Instant::now();
    let blocks = compare_lines(&left, &right, EXACT);
    assert!(
        start.elapsed() < Duration::from_secs(10),
        "{:?}",
        start.elapsed()
    );
    assert_eq!(20000, blocks.iter().map(|b| b.left_count).sum::<usize>());
    assert_eq!(20000, blocks.iter().map(|b| b.right_count).sum::<usize>());
}

// ---- InlineDiffer ----

#[test]
fn changed_word_is_highlighted_on_both_sides() {
    let (l, r) = compare_inline("int count = 5;", "int total = 5;", EXACT);
    assert_eq!(vec![TextRange::new(4, 5)], l);
    assert_eq!(vec![TextRange::new(4, 5)], r);
}

#[test]
fn inserted_word_is_only_on_one_side() {
    let (l, r) = compare_inline("a c", "a b c", EXACT);
    assert!(l.is_empty());
    assert_eq!(1, r.len());
    let s: String = "a b c".chars().skip(r[0].start).take(r[0].length).collect();
    assert_eq!("b ", s);
}

#[test]
fn whitespace_ranges_are_dropped_when_ignored() {
    let (l, r) = compare_inline(
        "a  b",
        "a b",
        TextDiffOptions {
            ignore_whitespace: true,
            ..EXACT
        },
    );
    assert!(l.is_empty());
    assert!(r.is_empty());
}

#[test]
fn inline_offsets_are_chars() {
    let (l, r) = compare_inline("ä = Grüße", "ä = Grüßx", EXACT);
    assert_eq!(vec![TextRange::new(4, 5)], l);
    assert_eq!(vec![TextRange::new(4, 5)], r);
}

#[test]
fn tokenize_splits_words_spaces_and_punctuation() {
    let s = "ab  c();";
    let tokens: Vec<String> = tokenize(s)
        .iter()
        .map(|t| s.chars().skip(t.start).take(t.length).collect())
        .collect();
    assert_eq!(vec!["ab", "  ", "c", "(", ")", ";"], tokens);
}

// ---- TextFileCodec ----

fn round_trip(bytes: &[u8]) -> LoadedText {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("t.txt");
    let b = dir.path().join("t2.txt");
    std::fs::write(&a, bytes).unwrap();
    let loaded = codec::load(&a).unwrap();
    codec::save(&b, &loaded.text, &loaded).unwrap();
    assert_eq!(bytes, std::fs::read(&b).unwrap().as_slice());
    loaded
}

#[test]
fn utf8_without_bom() {
    let t = round_trip("Grüße\r\nWelt".as_bytes());
    assert_eq!("Grüße\r\nWelt", t.text);
    assert_eq!("UTF-8", t.encoding_name());
    assert_eq!("\r\n", t.newline);
    assert_eq!("CRLF", t.newline_name());
}

#[test]
fn utf8_with_bom_keeps_bom() {
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(b"x\ny");
    let t = round_trip(&bytes);
    assert_eq!("x\ny", t.text);
    assert_eq!("UTF-8 BOM", t.encoding_name());
    assert_eq!("\n", t.newline);
}

#[test]
fn utf16_little_endian() {
    let mut bytes = vec![0xFF, 0xFE];
    bytes.extend("hällo\r\n".encode_utf16().flat_map(u16::to_le_bytes));
    let t = round_trip(&bytes);
    assert_eq!("hällo\r\n", t.text);
    assert!(!t.is_binary);
    assert_eq!("UTF-16 LE", t.encoding_name());
}

#[test]
fn utf16_big_endian() {
    let mut bytes = vec![0xFE, 0xFF];
    bytes.extend("hällo €".encode_utf16().flat_map(u16::to_be_bytes));
    let t = round_trip(&bytes);
    assert_eq!("hällo €", t.text);
    assert_eq!("UTF-16 BE", t.encoding_name());
}

#[test]
fn invalid_utf8_falls_back_to_ansi() {
    let bytes = [b'G', 0xFC, b't', 0x80]; // "Güt€" in Windows-1252
    let t = round_trip(&bytes);
    assert_eq!("Güt€", t.text);
    assert_eq!("ANSI", t.encoding_name());
}

#[test]
fn ansi_round_trips_all_bytes() {
    let bytes: Vec<u8> = (1u8..=255).filter(|&b| b != b'\r' && b != b'\n').collect();
    let t = round_trip(&bytes);
    assert_eq!("ANSI", t.encoding_name());
}

#[test]
fn ansi_unmappable_chars_become_question_marks() {
    let format = codec::decode(&[b'a', 0xFC]);
    assert_eq!(b"a?b".to_vec(), codec::encode("a文b", &format));
}

#[test]
fn nul_byte_is_binary() {
    assert!(codec::decode(&[1, 2, 0, 4]).is_binary);
    assert!(!codec::decode(b"plain").is_binary);
}

#[test]
fn detects_dominant_newline() {
    assert_eq!("\n", codec::detect_newline("a\nb\nc\r\n"));
    assert_eq!("\r\n", codec::detect_newline("a\r\nb\r\n"));
    assert_eq!("\r", codec::detect_newline("a\rb\r"));
    assert_eq!(DEFAULT_NEWLINE, codec::detect_newline("single line"));
}

#[test]
fn empty_like_keeps_format() {
    let other = codec::decode(&[0xEF, 0xBB, 0xBF, b'a', b'\n']);
    let e = LoadedText::empty_like(&other);
    assert_eq!("", e.text);
    assert_eq!("UTF-8 BOM", e.encoding_name());
    assert_eq!("\n", e.newline);
}

// ---- BlockCopy ----

fn apply(source: &str, target: &str, source_is_left: bool, nl: &str) -> String {
    let (left, right) = if source_is_left {
        (source, target)
    } else {
        (target, source)
    };
    let blocks = diff(left, right, EXACT);
    let diffs: Vec<&DiffBlock> = blocks.iter().filter(|b| b.is_difference()).collect();
    assert_eq!(1, diffs.len(), "{left:?} / {right:?}: {blocks:?}");
    let edit = compute_block_copy(
        &StringLineSource::new(source),
        &StringLineSource::new(target),
        diffs[0],
        source_is_left,
        nl,
    );
    edit.apply(target)
}

#[test]
fn copy_makes_target_equal_to_source() {
    let cases = [
        ("x\na\nb", "a\nb"),       // insert at start
        ("a\nx\nb", "a\nb"),       // insert in the middle
        ("a\nb\nx", "a\nb"),       // append after a last line without newline
        ("a\nb\nx\n", "a\nb\n"),   // insert before the final empty line
        ("a\nb", "a\nx\nb"),       // delete in the middle
        ("a\nb", "a\nb\nx"),       // delete at the end
        ("b", "x\nb"),             // delete at the start
        ("a\nX\nY\nb", "a\nq\nb"), // replace
        ("", "x\ny"),              // copy an empty file over
        ("x\ny", ""),              // copy into an empty file
        ("ä\nü", "ä\nö"),          // chars, not bytes
    ];
    for (source, target) in cases {
        assert_eq!(source, apply(source, target, true, "\n"), "{source:?}");
        assert_eq!(source, apply(source, target, false, "\n"), "{source:?}");
    }
}

#[test]
fn copied_line_breaks_are_converted_to_target_newline() {
    assert_eq!(
        "a\r\nX\r\nY\r\nb",
        apply("a\nX\nY\nb", "a\r\nq\r\nb", true, "\r\n")
    );
}

// ---- TextComparison ----

#[test]
fn fillers_make_both_sides_the_same_height() {
    let c = TextComparison::compute("a\nb\nc", "a\nX\nY\nZ\nc\nd", EXACT);
    let rows =
        |s: &SideInfo| s.line_count() + s.filler_above + s.filler_below.values().sum::<usize>();
    assert_eq!(c.total_rows, rows(&c.left));
    assert_eq!(c.total_rows, rows(&c.right));
    assert_eq!(2, c.left.filler_below(1)); // "b" vs "X Y Z": two filler rows below b
    assert_eq!(1, c.left.filler_below(2)); // "d" only on the right, after "c"
}

#[test]
fn insert_at_top_uses_filler_above() {
    let c = TextComparison::compute("b", "a\nb", EXACT);
    assert_eq!(1, c.left.filler_above);
    assert!(c.left.filler_below.is_empty());
}

#[test]
fn partner_line_follows_alignment() {
    let c = TextComparison::compute("a\nb\nc", "a\nc", EXACT);
    assert_eq!(Some(0), c.partner_line(true, 0));
    assert_eq!(None, c.partner_line(true, 1));
    assert_eq!(Some(1), c.partner_line(true, 2));
    assert_eq!(Some(2), c.partner_line(false, 1));
}

#[test]
fn block_at_and_row_of() {
    let c = TextComparison::compute("a\nb\nc", "a\nX\nY\nc", EXACT);
    assert_eq!(Some(1), c.block_at(true, 1));
    assert_eq!(Some(1), c.block_at(false, 2));
    assert_eq!(None, c.block_at(true, 9));
    assert_eq!(1, c.row_of(1));
    assert_eq!(3, c.row_of(2));
}

#[test]
fn changed_lines_get_inline_ranges() {
    let c = TextComparison::compute("x = 1", "x = 2", EXACT);
    assert_eq!(vec![TextRange::new(4, 1)], c.left.inline[&0]);
    assert_eq!(DiffBlockKind::Changed, c.right.kinds[0]);
    assert_eq!(1, c.difference_count);
}

#[test]
fn very_long_lines_get_no_inline_ranges() {
    let long_a = "a".repeat(MAX_INLINE_LINE_LENGTH + 1);
    let long_b = "b".repeat(MAX_INLINE_LINE_LENGTH + 1);
    let c = TextComparison::compute(&long_a, &long_b, EXACT);
    assert!(c.left.inline.is_empty());
    assert_eq!(DiffBlockKind::Changed, c.left.kinds[0]);
}
