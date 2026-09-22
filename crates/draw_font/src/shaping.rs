//! Bidi run splitting and script detection — pure and font-independent, so
//! they are unit-testable without a font.

use std::ops::Range;

use rustybuzz::Direction;
use unicode_bidi::BidiInfo;

/// Splits `text` into bidi runs in visual order, with `(byte range, is_rtl)`.
pub(crate) fn bidi_runs(text: &str) -> Vec<(Range<usize>, bool)> {
    if text.is_empty() {
        return Vec::new();
    }
    let bidi = BidiInfo::new(text, None);
    let mut runs = Vec::new();
    for para in &bidi.paragraphs {
        if para.range.is_empty() {
            continue;
        }
        let (levels, visual) = bidi.visual_runs(para, para.range.clone());
        for run in visual {
            if run.is_empty() {
                continue;
            }
            let rtl = levels[run.start].is_rtl();
            runs.push((run, rtl));
        }
    }
    runs
}

/// The rustybuzz direction for `rtl`.
pub(crate) fn direction(rtl: bool) -> Direction {
    if rtl {
        Direction::RightToLeft
    } else {
        Direction::LeftToRight
    }
}

/// Best script for `text`: the first character that is not Common/Inherited.
pub(crate) fn run_script(text: &str) -> rustybuzz::Script {
    for ch in text.chars() {
        let script = unicode_script::Script::from(ch);
        if script != unicode_script::Script::Common && script != unicode_script::Script::Inherited {
            return script_from_name(script.short_name());
        }
    }
    rustybuzz::script::LATIN
}

fn script_from_name(name: &str) -> rustybuzz::Script {
    let bytes = name.as_bytes();
    if let [a, b, c, d] = bytes {
        let tag = rustybuzz::ttf_parser::Tag::from_bytes(&[*a, *b, *c, *d]);
        if let Some(script) = rustybuzz::Script::from_iso15924_tag(tag) {
            return script;
        }
    }
    rustybuzz::script::LATIN
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bidi_runs_reorder_rtl_around_latin() {
        // Logical: "abc" then Hebrew "אבג". Visual order keeps "abc" first
        // (LTR base) and the Hebrew run reversed as one block.
        let text = "abc\u{5D0}\u{5D1}\u{5D2}";
        let runs = bidi_runs(text);
        let joined: String = runs.iter().map(|(r, _)| &text[r.clone()]).collect();
        assert_eq!(joined, text, "runs must cover the text");
        assert!(!runs[0].1, "leading Latin run is LTR");
        assert!(runs.last().unwrap().1, "Hebrew run is RTL");
        assert_eq!(
            &text[runs.last().unwrap().0.clone()],
            "\u{5D0}\u{5D1}\u{5D2}"
        );
    }

    #[test]
    fn bidi_runs_pure_ltr_is_one_run() {
        let runs = bidi_runs("hello");
        assert_eq!(runs.len(), 1);
        assert!(!runs[0].1);
    }

    #[test]
    fn script_detection_prefers_strong_characters() {
        assert_eq!(run_script("123abc"), rustybuzz::script::LATIN);
        assert_eq!(run_script("\u{5D0}"), rustybuzz::script::HEBREW);
    }
}
