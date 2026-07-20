use crate::error::{CheckError, CheckResult};

use ec4rs::{
    Properties,
    property::{
        Charset, EndOfLine, FinalNewline, IndentSize, IndentStyle, MaxLineLen, TabWidth,
        TrimTrailingWs,
    },
};
use memchr::memchr_iter;
use memchr::memmem;

const EDCFG_LINT_PREFIX: &str = "edcfg-lint-";
const EDCFG_LINT_SKIP_FILE: &str = "edcfg-lint-disable-file";
const EDCFG_LINT_SKIP_NEXT_LINE: &str = "edcfg-lint-disable-next-line";
const EDCFG_LINT_SKIP_THIS_LINE: &str = "edcfg-lint-disable-line";
const EDCFG_LINT_SKIP_DISABLE_BLOCK: &str = "edcfg-lint-off";
const EDCFG_LINT_SKIP_ENABLE_BLOCK: &str = "edcfg-lint-on";

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
enum DeferredSkipState {
    NextLine,
    DisabledBlock,
}

fn first_non_whitespace_or_tab_pos(line: &str) -> Option<usize> {
    line.bytes().position(|b| b != b' ' && b != b'\t')
}

fn line_space_width(line: &str, tab_width: usize) -> usize {
    // skip over consecutive runs of `tabs_or_spaces` positions, on the theory the first non-whitespace/tab character will
    // occur after a run of positions
    line.bytes()
        .take_while(|&b| b == b' ' || b == b'\t')
        .map(|b| if b == b'\t' { tab_width } else { 1 })
        .sum()
}

/// Extracted EditorConfig properties that are checked per-line.
/// This allows us to amortize the property lookups across all lines in a file.
struct LineCheckConfig {
    tab_width: Option<usize>,
    indent_style: Option<IndentStyle>,
    trim_trailing_ws: Option<bool>,
    max_line_len: Option<usize>,
}

impl LineCheckConfig {
    fn from_properties(properties: &Properties) -> Self {
        let raw_tab_width = properties.get::<TabWidth>().ok().and_then(|res| match res {
            TabWidth::Value(0) => None,
            TabWidth::Value(val) => Some(val),
        });

        // There is one invalid case here: suppose `indent_size = tab` and `tab_width` is unset(!)
        // we'll handle it for now by treating as "don't check it at all", but perhaps it's worth
        // emitting an error.
        let indent_width = properties
            .get::<IndentSize>()
            .ok()
            .and_then(|res| match res {
                IndentSize::Value(0) => None,
                IndentSize::Value(val) => Some(val),
                IndentSize::UseTabWidth => raw_tab_width,
            });

        // Tab width defaults to `indent_size` if tab width isn't specified but indent_size is.
        let tab_width = raw_tab_width.or(indent_width);

        let indent_style = properties.get::<IndentStyle>().ok();
        let trim_trailing_ws = properties
            .get::<TrimTrailingWs>()
            .ok()
            .map(|res| match res {
                TrimTrailingWs::Value(val) => val,
            });
        let max_line_len = properties
            .get::<MaxLineLen>()
            .ok()
            .and_then(|res| match res {
                MaxLineLen::Value(len) => Some(len),
                MaxLineLen::Off => None,
            });

        LineCheckConfig {
            tab_width,
            indent_style,
            trim_trailing_ws,
            max_line_len,
        }
    }
}

fn check_editorconfig_properties_for_line(
    cur_line_num: usize,
    cur_line: &str,
    config: &LineCheckConfig,
    errors: &mut CheckResult,
) {
    if let Some(indent_style) = config.indent_style {
        let leading_whitespace_or_tabs_str = first_non_whitespace_or_tab_pos(cur_line)
            .map(|pos| &cur_line[0..pos])
            .unwrap_or(cur_line);
        let spaces = memchr_iter(b' ', leading_whitespace_or_tabs_str.as_bytes()).count();
        let tabs = memchr_iter(b'\t', leading_whitespace_or_tabs_str.as_bytes()).count();

        if let Some(tab_width) = config.tab_width {
            let cur_line_width: usize = line_space_width(cur_line, tab_width);

            // check indentation style
            let (desired_tabs, desired_spaces) = match indent_style {
                IndentStyle::Spaces => (0, cur_line_width),
                IndentStyle::Tabs => (
                    cur_line_width.div_euclid(tab_width),
                    cur_line_width.rem_euclid(tab_width),
                ),
            };

            if desired_tabs != tabs || spaces != desired_spaces {
                errors.push(CheckError::WrongIndentStyle {
                    line: cur_line_num,
                    expected: indent_style,
                    expected_tabs: desired_tabs,
                    expected_spaces: desired_spaces,
                    actual_spaces: spaces,
                    actual_tabs: tabs,
                });
            }
        } else {
            let (desired_tabs, desired_spaces) = match indent_style {
                IndentStyle::Spaces => (0, spaces),
                IndentStyle::Tabs => (tabs, 0),
            };
            if desired_tabs != tabs || spaces != desired_spaces {
                errors.push(CheckError::WrongIndentStyleBasic {
                    line: cur_line_num,
                    expected: indent_style,
                    actual_spaces: spaces,
                    actual_tabs: tabs,
                });
            }
        }
    }

    if config.trim_trailing_ws.is_some_and(|val| val)
        && let Some(last_char) = cur_line.chars().next_back()
        && (last_char == ' ' || last_char == '\t')
    {
        errors.push(CheckError::TrailingWhitespace { line: cur_line_num });
    }

    if let Some(max_line_len) = config.max_line_len {
        let line_len = cur_line.chars().count();
        if line_len > max_line_len {
            errors.push(CheckError::LineTooLong {
                line: cur_line_num,
                actual_length: line_len,
                max_length: max_line_len,
            });
        }
    }
}

fn check_editorconfig_line_endings(
    contents: &str,
    properties: &Properties,
    empty_file_passes: bool,
    errors: &mut CheckResult,
) {
    if empty_file_passes && contents.is_empty() {
        return;
    }

    if properties.get::<EndOfLine>().is_err() {
        return;
    }

    let line_ending_mode = properties.get::<EndOfLine>().unwrap();
    let desired_le = match line_ending_mode {
        EndOfLine::Cr => "\r",
        EndOfLine::Lf => "\n",
        EndOfLine::CrLf => "\r\n",
    };

    let content_bytes = contents.as_bytes();

    let crlfs = memmem::find_iter(content_bytes, b"\r\n").count();
    let crs = memchr_iter(b'\r', content_bytes).count();
    let lfs = memchr_iter(b'\n', content_bytes).count();

    let line_endings_match = match line_ending_mode {
        EndOfLine::Cr => lfs == 0,
        EndOfLine::Lf => crs == 0,
        EndOfLine::CrLf => crs == crlfs && lfs == crlfs,
    };

    if !line_endings_match {
        errors.push(CheckError::WrongLineEnding {
            expected: desired_le.escape_unicode().to_string(),
        });
    }

    if let Some(FinalNewline::Value(final_newline)) = properties.get::<FinalNewline>().ok()
        && final_newline
    {
        let desired_le_len = desired_le.len();
        if contents.len() < desired_le_len || !contents.ends_with(desired_le) {
            errors.push(CheckError::MissingFinalNewline);
        }
    }
}

pub fn check_file_against_editorconfig(contents: &[u8], properties: &Properties) -> CheckResult {
    let mut errors = vec![];

    // Validate that the file is indeed encoded as expected.
    let charset = properties.get::<Charset>().unwrap_or(Charset::Utf8);
    let specified_encoding = match charset {
        Charset::Utf8 => encoding_rs::UTF_8,
        Charset::Utf8Bom => encoding_rs::UTF_8,
        Charset::Latin1 => encoding_rs::WINDOWS_1252,
        Charset::Utf16Le => encoding_rs::UTF_16LE,
        Charset::Utf16Be => encoding_rs::UTF_16BE,
    };

    // Check for the presence of a BOM
    let bom_present = encoding_rs::Encoding::for_bom(contents).is_some();

    let (decoded_string, sniffed_encoding, replacements) = specified_encoding.decode(contents);
    if sniffed_encoding != specified_encoding {
        let reverse_encoding = if sniffed_encoding == encoding_rs::UTF_8 {
            Charset::Utf8
        } else if sniffed_encoding == encoding_rs::UTF_16LE {
            Charset::Utf16Le
        } else if sniffed_encoding == encoding_rs::UTF_16BE {
            Charset::Utf16Be
        } else if sniffed_encoding == encoding_rs::WINDOWS_1252 {
            Charset::Latin1
        } else {
            unreachable!()
        };
        errors.push(CheckError::WrongFileEncoding {
            expected: charset,
            actual: reverse_encoding,
        });
    }
    // Special case: we *can* decode the string as UTF-8, but a BOM was present
    if sniffed_encoding == encoding_rs::UTF_8 {
        if !bom_present && charset == Charset::Utf8Bom {
            errors.push(CheckError::WrongFileEncoding {
                expected: Charset::Utf8Bom,
                actual: Charset::Utf8,
            });
        } else if bom_present && charset == Charset::Utf8 {
            errors.push(CheckError::WrongFileEncoding {
                expected: Charset::Utf8,
                actual: Charset::Utf8Bom,
            });
        }
    }
    if replacements {
        errors.push(CheckError::IncorrectFileEncoding { charset });
    }

    if let Some(first_line) = decoded_string.lines().next()
        && first_line.contains(EDCFG_LINT_SKIP_FILE)
    {
        return vec![];
    }

    check_editorconfig_line_endings(&decoded_string, properties, true, &mut errors);

    // Extract properties once for all lines to amortize hashmap lookups. The benchmark-only
    // variant below reconstructs the same values per checked line without changing semantics.
    #[cfg(not(feature = "bench-per-line-properties"))]
    let line_config = LineCheckConfig::from_properties(properties);

    // Set up logic for skipping code as needed
    let mut deferred_skip_state: Option<DeferredSkipState> = None;
    let edcfg_lint_prefix_finder = memmem::Finder::new(EDCFG_LINT_PREFIX.as_bytes());

    for (i, line) in decoded_string.lines().enumerate() {
        if deferred_skip_state.is_none() {
            if edcfg_lint_prefix_finder.find(line.as_bytes()).is_some() {
                if line.contains(EDCFG_LINT_SKIP_THIS_LINE) {
                    continue;
                } else if line.contains(EDCFG_LINT_SKIP_NEXT_LINE) {
                    deferred_skip_state = Some(DeferredSkipState::NextLine);
                } else if line.contains(EDCFG_LINT_SKIP_DISABLE_BLOCK) {
                    deferred_skip_state = Some(DeferredSkipState::DisabledBlock);
                }
            }
        } else {
            match deferred_skip_state.unwrap() {
                DeferredSkipState::NextLine => {
                    deferred_skip_state = None;
                    continue;
                }
                DeferredSkipState::DisabledBlock => {
                    if line.contains(EDCFG_LINT_SKIP_ENABLE_BLOCK) {
                        deferred_skip_state = None;
                    }
                    continue;
                }
            }
        }

        #[cfg(feature = "bench-per-line-properties")]
        let line_config = LineCheckConfig::from_properties(properties);

        check_editorconfig_properties_for_line(i + 1, line, &line_config, &mut errors);
    }
    errors
}

#[cfg(test)]
mod test {
    use super::*;
    use ec4rs::property::IndentSize;

    #[test]
    fn test_first_non_whitespace_or_tab_pos() {
        assert_eq!(Some(5), first_non_whitespace_or_tab_pos("     potato"));
        assert_eq!(Some(1), first_non_whitespace_or_tab_pos("\tpotato"));
        assert_eq!(Some(3), first_non_whitespace_or_tab_pos("\t  potato"));
        assert_eq!(Some(0), first_non_whitespace_or_tab_pos("potato"));
        assert_eq!(None, first_non_whitespace_or_tab_pos(" "));
        assert_eq!(None, first_non_whitespace_or_tab_pos("  "));
    }

    #[test]
    fn test_line_space_width() {
        assert_eq!(5, line_space_width("     potato", 4));
        assert_eq!(4, line_space_width("\tpotato", 4));
        assert_eq!(6, line_space_width("\t  potato", 4));
        assert_eq!(0, line_space_width("potato", 4));
        assert_eq!(8, line_space_width("\t\tpotato", 4));
        assert_eq!(10, line_space_width("          potato", 4));
    }

    #[test]
    fn test_check_indent_size_valid() {
        let mut properties = Properties::default();
        properties.insert(IndentSize::Value(4));
        properties.insert(TabWidth::Value(4));
        properties.insert(IndentStyle::Spaces);

        let config = LineCheckConfig::from_properties(&properties);
        let mut errors = vec![];
        check_editorconfig_properties_for_line(1, "    code", &config, &mut errors);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_check_indent_style_spaces() {
        let mut properties = Properties::default();
        properties.insert(IndentSize::Value(4));
        properties.insert(TabWidth::Value(4));
        properties.insert(IndentStyle::Spaces);

        let config = LineCheckConfig::from_properties(&properties);
        let mut errors = vec![];
        check_editorconfig_properties_for_line(1, "    code", &config, &mut errors);
        assert!(errors.is_empty());

        let mut errors = vec![];
        check_editorconfig_properties_for_line(1, "\tcode", &config, &mut errors);
        assert_eq!(errors.len(), 1);
        match &errors[0] {
            CheckError::WrongIndentStyle { expected, .. } => {
                assert_eq!(*expected, IndentStyle::Spaces);
            }
            _ => panic!("Expected WrongIndentStyle error"),
        }
    }

    #[test]
    fn test_check_indent_style_tabs() {
        let mut properties = Properties::default();
        properties.insert(IndentSize::Value(4));
        properties.insert(TabWidth::Value(4));
        properties.insert(IndentStyle::Tabs);

        let config = LineCheckConfig::from_properties(&properties);
        let mut errors = vec![];
        check_editorconfig_properties_for_line(1, "\tcode", &config, &mut errors);
        assert!(errors.is_empty());

        let mut errors = vec![];
        check_editorconfig_properties_for_line(1, "    code", &config, &mut errors);
        assert_eq!(errors.len(), 1);
        match &errors[0] {
            CheckError::WrongIndentStyle { expected, .. } => {
                assert_eq!(*expected, IndentStyle::Tabs);
            }
            _ => panic!("Expected WrongIndentStyle error"),
        }
    }

    #[test]
    fn test_check_indent_style_without_a_width() {
        let mut spaces_properties = Properties::default();
        spaces_properties.insert(IndentStyle::Spaces);
        let spaces_config = LineCheckConfig::from_properties(&spaces_properties);
        let mut spaces_errors = vec![];
        check_editorconfig_properties_for_line(1, "\tcode", &spaces_config, &mut spaces_errors);
        assert!(matches!(
            spaces_errors.as_slice(),
            [CheckError::WrongIndentStyleBasic {
                expected: IndentStyle::Spaces,
                ..
            }]
        ));

        let mut tabs_properties = Properties::default();
        tabs_properties.insert(IndentStyle::Tabs);
        let tabs_config = LineCheckConfig::from_properties(&tabs_properties);
        let mut tabs_errors = vec![];
        check_editorconfig_properties_for_line(1, "    code", &tabs_config, &mut tabs_errors);
        assert!(matches!(
            tabs_errors.as_slice(),
            [CheckError::WrongIndentStyleBasic {
                expected: IndentStyle::Tabs,
                ..
            }]
        ));
    }

    #[test]
    fn test_indent_size_is_only_a_fallback_for_tab_width() {
        let mut properties = Properties::default();
        properties.insert(IndentSize::Value(4));
        properties.insert(IndentStyle::Tabs);
        let config = LineCheckConfig::from_properties(&properties);

        let mut valid_errors = vec![];
        check_editorconfig_properties_for_line(1, "\tcode", &config, &mut valid_errors);
        assert!(valid_errors.is_empty());

        let mut invalid_errors = vec![];
        check_editorconfig_properties_for_line(1, "    code", &config, &mut invalid_errors);
        assert!(matches!(
            invalid_errors.as_slice(),
            [CheckError::WrongIndentStyle {
                expected: IndentStyle::Tabs,
                ..
            }]
        ));
    }

    #[test]
    fn test_check_trailing_whitespace() {
        let mut properties = Properties::default();
        properties.insert(TrimTrailingWs::Value(true));
        properties.insert(IndentStyle::Spaces);

        let config = LineCheckConfig::from_properties(&properties);
        let mut errors = vec![];
        check_editorconfig_properties_for_line(1, "code  ", &config, &mut errors);
        assert_eq!(errors.len(), 1);
        match &errors[0] {
            CheckError::TrailingWhitespace { line } => {
                assert_eq!(*line, 1);
            }
            _ => panic!("Expected TrailingWhitespace error"),
        }

        let mut errors = vec![];
        check_editorconfig_properties_for_line(1, "code", &config, &mut errors);
        assert!(
            errors
                .iter()
                .all(|e| !matches!(e, CheckError::TrailingWhitespace { .. }))
        );
    }

    #[test]
    fn test_check_max_line_length() {
        let mut properties = Properties::default();
        properties.insert(MaxLineLen::Value(10));
        properties.insert(IndentStyle::Spaces);

        let config = LineCheckConfig::from_properties(&properties);
        let mut errors = vec![];
        check_editorconfig_properties_for_line(1, "short line", &config, &mut errors);
        assert!(
            errors
                .iter()
                .all(|e| !matches!(e, CheckError::LineTooLong { .. }))
        );

        let mut errors = vec![];
        check_editorconfig_properties_for_line(1, "this is a very long line", &config, &mut errors);
        assert_eq!(
            errors
                .iter()
                .filter(|e| matches!(e, CheckError::LineTooLong { .. }))
                .count(),
            1
        );
        match errors
            .iter()
            .find(|e| matches!(e, CheckError::LineTooLong { .. }))
        {
            Some(CheckError::LineTooLong {
                line,
                actual_length,
                max_length,
            }) => {
                assert_eq!(*line, 1);
                assert_eq!(*actual_length, 24);
                assert_eq!(*max_length, 10);
            }
            _ => panic!("Expected LineTooLong error"),
        }
    }

    #[test]
    fn test_check_line_endings_empty() {
        let mut properties = Properties::default();
        properties.insert(EndOfLine::Lf);
        properties.insert(FinalNewline::Value(true));

        let mut errors = vec![];
        check_editorconfig_line_endings("", &properties, true, &mut errors);
        assert!(errors.is_empty());

        let mut errors = vec![];
        check_editorconfig_line_endings("", &properties, false, &mut errors);
        assert_eq!(errors.len(), 1);
        match &errors[0] {
            CheckError::MissingFinalNewline => (),
            _ => panic!("Expected WrongLineEnding error"),
        }
    }

    #[test]
    fn test_check_line_endings_lf() {
        let mut properties = Properties::default();
        properties.insert(EndOfLine::Lf);

        let mut errors = vec![];
        check_editorconfig_line_endings("line1\nline2\n", &properties, true, &mut errors);
        assert!(errors.is_empty());

        let mut errors = vec![];
        check_editorconfig_line_endings("line1\r\nline2\r\n", &properties, true, &mut errors);
        assert_eq!(errors.len(), 1);
        match &errors[0] {
            CheckError::WrongLineEnding { expected } => {
                assert_eq!(expected, "\\u{a}");
            }
            _ => panic!("Expected WrongLineEnding error"),
        }
    }

    #[test]
    fn test_check_line_endings_crlf() {
        let mut properties = Properties::default();
        properties.insert(EndOfLine::CrLf);

        let mut errors = vec![];
        check_editorconfig_line_endings("line1\r\nline2\r\n", &properties, true, &mut errors);
        assert!(errors.is_empty());

        let mut errors = vec![];
        check_editorconfig_line_endings("line1\nline2\n", &properties, true, &mut errors);
        assert_eq!(errors.len(), 1);
        match &errors[0] {
            CheckError::WrongLineEnding { expected } => {
                assert_eq!(expected, "\\u{d}\\u{a}");
            }
            _ => panic!("Expected WrongLineEnding error"),
        }

        let mut errors = vec![];
        check_editorconfig_line_endings("line1\n\rline2\n\r", &properties, true, &mut errors);
        assert_eq!(errors.len(), 1);
        match &errors[0] {
            CheckError::WrongLineEnding { expected } => {
                assert_eq!(expected, "\\u{d}\\u{a}");
            }
            _ => panic!("Expected WrongLineEnding error"),
        }
    }

    #[test]
    fn test_check_line_endings_cr() {
        let mut properties = Properties::default();
        properties.insert(EndOfLine::Cr);

        let mut errors = vec![];
        check_editorconfig_line_endings("line1\rline2\r", &properties, true, &mut errors);
        assert!(errors.is_empty());

        let mut errors = vec![];
        check_editorconfig_line_endings("line1\nline2\n", &properties, true, &mut errors);
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn test_check_final_newline_present() {
        let mut properties = Properties::default();
        properties.insert(EndOfLine::Lf);
        properties.insert(FinalNewline::Value(true));

        let mut errors = vec![];
        check_editorconfig_line_endings("line1\nline2\n", &properties, true, &mut errors);
        assert!(
            errors
                .iter()
                .all(|e| !matches!(e, CheckError::MissingFinalNewline))
        );
    }

    #[test]
    fn test_check_final_newline_missing() {
        let mut properties = Properties::default();
        properties.insert(EndOfLine::Lf);
        properties.insert(FinalNewline::Value(true));

        let mut errors = vec![];
        check_editorconfig_line_endings("line1\nline2", &properties, true, &mut errors);
        assert_eq!(
            errors
                .iter()
                .filter(|e| matches!(e, CheckError::MissingFinalNewline))
                .count(),
            1
        );
    }

    #[test]
    fn test_check_file_integration() {
        let mut properties = Properties::default();
        properties.insert(IndentSize::Value(2));
        properties.insert(TabWidth::Value(4));
        properties.insert(IndentStyle::Spaces);
        properties.insert(TrimTrailingWs::Value(true));
        properties.insert(EndOfLine::Lf);
        properties.insert(FinalNewline::Value(true));

        // Valid file
        let errors = check_file_against_editorconfig(b"  line1\n  line2\n", &properties);
        assert!(errors.is_empty());

        // File with multiple errors
        let errors = check_file_against_editorconfig(b"   line1  \n\tline2", &properties);
        assert!(errors.len() > 1);
    }

    #[test]
    fn test_charset_utf8_no_bom() {
        let mut properties = Properties::default();
        properties.insert(Charset::Utf8);

        let errors = check_file_against_editorconfig(b"Hello UTF-8\n", &properties);
        assert!(
            errors.is_empty(),
            "UTF-8 without BOM should pass for charset=utf-8"
        );
    }

    #[test]
    fn test_charset_utf8_with_bom_correct() {
        let mut properties = Properties::default();
        properties.insert(Charset::Utf8Bom);

        let content = b"\xEF\xBB\xBFHello UTF-8 with BOM\n";
        let errors = check_file_against_editorconfig(content, &properties);
        assert!(
            errors.is_empty(),
            "UTF-8 with BOM should pass for charset=utf-8-bom"
        );
    }

    #[test]
    fn test_charset_utf8_unexpected_bom() {
        let mut properties = Properties::default();
        properties.insert(Charset::Utf8);

        let content = b"\xEF\xBB\xBFHello UTF-8 with BOM\n";
        let errors = check_file_against_editorconfig(content, &properties);
        assert_eq!(
            errors.len(),
            1,
            "UTF-8 with BOM should fail for charset=utf-8"
        );
        match &errors[0] {
            CheckError::WrongFileEncoding { expected, actual } => {
                assert_eq!(*expected, Charset::Utf8);
                assert_eq!(*actual, Charset::Utf8Bom);
            }
            _ => panic!("Expected WrongFileEncoding error"),
        }
    }

    #[test]
    fn test_charset_utf8_missing_bom() {
        let mut properties = Properties::default();
        properties.insert(Charset::Utf8Bom);

        let content = b"Hello UTF-8 without BOM\n";
        let errors = check_file_against_editorconfig(content, &properties);
        assert_eq!(
            errors.len(),
            1,
            "UTF-8 without BOM should fail for charset=utf-8-bom"
        );
        match &errors[0] {
            CheckError::WrongFileEncoding { expected, actual } => {
                assert_eq!(*expected, Charset::Utf8Bom);
                assert_eq!(*actual, Charset::Utf8);
            }
            _ => panic!("Expected WrongFileEncoding error"),
        }
    }

    #[test]
    fn test_charset_utf16le_correct() {
        let mut properties = Properties::default();
        properties.insert(Charset::Utf16Le);

        // UTF-16LE BOM (FF FE) + "Hi\n" in UTF-16LE
        let content = b"\xFF\xFEH\x00i\x00\n\x00";
        let errors = check_file_against_editorconfig(content, &properties);
        assert!(
            errors.is_empty(),
            "UTF-16LE with BOM should pass for charset=utf-16le"
        );
    }

    #[test]
    fn test_charset_utf16le_claimed_as_utf8() {
        let mut properties = Properties::default();
        properties.insert(Charset::Utf8);

        // UTF-16LE BOM (FF FE) + "Hi\n" in UTF-16LE
        let content = b"\xFF\xFEH\x00i\x00\n\x00";
        let errors = check_file_against_editorconfig(content, &properties);
        assert_eq!(
            errors.len(),
            1,
            "UTF-16LE should fail when claimed as UTF-8"
        );
        match &errors[0] {
            CheckError::WrongFileEncoding { expected, actual } => {
                assert_eq!(*expected, Charset::Utf8);
                assert_eq!(*actual, Charset::Utf16Le);
            }
            _ => panic!("Expected WrongFileEncoding error"),
        }
    }

    #[test]
    fn test_charset_utf16be_correct() {
        let mut properties = Properties::default();
        properties.insert(Charset::Utf16Be);

        // UTF-16BE BOM (FE FF) + "Hi\n" in UTF-16BE
        let content = b"\xFE\xFF\x00H\x00i\x00\n";
        let errors = check_file_against_editorconfig(content, &properties);
        assert!(
            errors.is_empty(),
            "UTF-16BE with BOM should pass for charset=utf-16be"
        );
    }

    #[test]
    fn test_charset_latin1_correct() {
        let mut properties = Properties::default();
        properties.insert(Charset::Latin1);

        // Latin1: "Café\n" with é as 0xE9
        let content = b"Caf\xE9\n";
        let errors = check_file_against_editorconfig(content, &properties);
        assert!(
            errors.is_empty(),
            "Latin1 content should pass for charset=latin1"
        );
    }

    #[test]
    fn test_charset_malformed_utf8() {
        let mut properties = Properties::default();
        properties.insert(Charset::Utf8);

        // Invalid UTF-8 sequence
        let content = b"Hello\xFF\xFEWorld\n";
        let errors = check_file_against_editorconfig(content, &properties);

        // Should detect either wrong encoding (sniffed as UTF-16LE due to FF FE)
        // or incorrect encoding (replacements needed)
        assert!(!errors.is_empty(), "Malformed UTF-8 should produce errors");
        assert!(
            errors.iter().any(|e| matches!(
                e,
                CheckError::WrongFileEncoding { .. } | CheckError::IncorrectFileEncoding { .. }
            )),
            "Should detect encoding issue"
        );
    }

    #[test]
    fn test_skip_file_directive() {
        let mut properties = Properties::default();
        properties.insert(IndentStyle::Spaces);
        properties.insert(IndentSize::Value(4));
        properties.insert(TrimTrailingWs::Value(true));

        // File with edcfg-lint-disable-file on first line should skip all checks
        let content = b"// edcfg-lint-disable-file\n\tindent with tabs  \n";
        let errors = check_file_against_editorconfig(content, &properties);
        assert!(
            errors.is_empty(),
            "edcfg-lint-disable-file should skip all checks"
        );

        // Same content without the directive should produce errors
        let content_no_skip = b"// some comment\n\tindent with tabs  \n";
        let errors = check_file_against_editorconfig(content_no_skip, &properties);
        assert!(
            !errors.is_empty(),
            "Without edcfg-lint-disable-file, errors should be reported"
        );
    }

    #[test]
    fn test_skip_file_directive_must_be_first_line() {
        let mut properties = Properties::default();
        properties.insert(IndentStyle::Spaces);
        properties.insert(IndentSize::Value(4));
        properties.insert(TrimTrailingWs::Value(true));

        // edcfg-lint-disable-file on second line should NOT skip the file
        let content = b"// some comment\n// edcfg-lint-disable-file\n\tindent with tabs  \n";
        let errors = check_file_against_editorconfig(content, &properties);
        assert!(
            !errors.is_empty(),
            "edcfg-lint-disable-file on non-first line should not skip file"
        );
    }

    #[test]
    fn test_skip_this_line_directive() {
        let mut properties = Properties::default();
        properties.insert(IndentStyle::Spaces);
        properties.insert(IndentSize::Value(4));
        properties.insert(TrimTrailingWs::Value(true));

        // Line with edcfg-lint-disable-line should be skipped
        let content = b"\tbad indent // edcfg-lint-disable-line\ngood line\n";
        let errors = check_file_against_editorconfig(content, &properties);
        assert!(
            errors.is_empty(),
            "edcfg-lint-disable-line should skip checking that line"
        );

        // Same content without directive should produce error
        let content_no_skip = b"\tbad indent // some comment\ngood line\n";
        let errors = check_file_against_editorconfig(content_no_skip, &properties);
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, CheckError::WrongIndentStyle { line: 1, .. })),
            "Without edcfg-lint-disable-line, indent error should be reported"
        );
    }

    #[test]
    fn test_skip_next_line_directive() {
        let mut properties = Properties::default();
        properties.insert(IndentStyle::Spaces);
        properties.insert(IndentSize::Value(4));
        properties.insert(TrimTrailingWs::Value(true));

        // Line after edcfg-lint-disable-next-line should be skipped
        let content = b"// edcfg-lint-disable-next-line\n\tbad indent  \ngood line\n";
        let errors = check_file_against_editorconfig(content, &properties);
        assert!(
            errors.is_empty(),
            "edcfg-lint-disable-next-line should skip checking the following line"
        );

        // Same content without directive should produce errors
        let content_no_skip = b"// some comment\n\tbad indent  \ngood line\n";
        let errors = check_file_against_editorconfig(content_no_skip, &properties);
        assert!(
            !errors.is_empty(),
            "Without edcfg-lint-disable-next-line, errors should be reported"
        );
    }

    #[test]
    fn test_skip_next_line_only_skips_one_line() {
        let mut properties = Properties::default();
        properties.insert(IndentStyle::Spaces);
        properties.insert(IndentSize::Value(4));
        properties.insert(TrimTrailingWs::Value(true));

        // edcfg-lint-disable-next-line should only skip the immediately following line
        let content = b"// edcfg-lint-disable-next-line\n\tskipped line\n\tnot skipped\n";
        let errors = check_file_against_editorconfig(content, &properties);
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, CheckError::WrongIndentStyle { line: 3, .. })),
            "edcfg-lint-disable-next-line should only skip one line"
        );
    }

    #[test]
    fn test_skip_block_directive() {
        let mut properties = Properties::default();
        properties.insert(IndentStyle::Spaces);
        properties.insert(IndentSize::Value(4));
        properties.insert(TrimTrailingWs::Value(true));

        // Lines between edcfg-lint-off and edcfg-lint-on should be skipped
        let content =
            b"good line\n// edcfg-lint-off\n\tbad line 1  \n\tbad line 2  \n// edcfg-lint-on\ngood line\n";
        let errors = check_file_against_editorconfig(content, &properties);
        assert!(
            errors.is_empty(),
            "Lines between edcfg-lint-off and edcfg-lint-on should be skipped"
        );

        // Same content without directives should produce errors
        let content_no_skip =
            b"good line\n// comment\n\tbad line 1  \n\tbad line 2  \n// comment\ngood line\n";
        let errors = check_file_against_editorconfig(content_no_skip, &properties);
        assert!(
            !errors.is_empty(),
            "Without edcfg-lint-off/edcfg-lint-on, errors should be reported"
        );
    }

    #[test]
    fn test_skip_block_without_enable() {
        let mut properties = Properties::default();
        properties.insert(IndentStyle::Spaces);
        properties.insert(IndentSize::Value(4));
        properties.insert(TrimTrailingWs::Value(true));

        // edcfg-lint-off without edcfg-lint-on should skip all remaining lines
        let content = b"good line\n// edcfg-lint-off\n\tbad line 1  \n\tbad line 2  \n";
        let errors = check_file_against_editorconfig(content, &properties);
        assert!(
            errors.is_empty(),
            "edcfg-lint-off without edcfg-lint-on should skip all remaining lines"
        );
    }

    #[test]
    fn test_skip_directives_in_various_comment_styles() {
        let mut properties = Properties::default();
        properties.insert(IndentStyle::Spaces);
        properties.insert(IndentSize::Value(4));
        properties.insert(TrimTrailingWs::Value(true));

        // edcfg-lint-disable-line in different comment styles
        let content_c_style = b"\tbad indent /* edcfg-lint-disable-line */\n";
        let errors = check_file_against_editorconfig(content_c_style, &properties);
        assert!(
            errors.is_empty(),
            "edcfg-lint-disable-line should work in C-style comments"
        );

        let content_hash = b"\tbad indent # edcfg-lint-disable-line\n";
        let errors = check_file_against_editorconfig(content_hash, &properties);
        assert!(
            errors.is_empty(),
            "edcfg-lint-disable-line should work with hash comments"
        );
    }

    #[test]
    fn test_multiple_skip_blocks() {
        let mut properties = Properties::default();
        properties.insert(IndentStyle::Spaces);
        properties.insert(IndentSize::Value(4));
        properties.insert(TrimTrailingWs::Value(true));

        // Multiple edcfg-lint-off/edcfg-lint-on blocks
        let content =
            b"good\n// edcfg-lint-off\n\tbad\n// edcfg-lint-on\ngood\n// edcfg-lint-off\n\tbad\n// edcfg-lint-on\ngood\n";
        let errors = check_file_against_editorconfig(content, &properties);
        assert!(
            errors.is_empty(),
            "Multiple edcfg-lint-off/edcfg-lint-on blocks should all be respected"
        );
    }
}
