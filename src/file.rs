use crate::error::{CheckError, CheckResult};

use ec4rs::{
    Properties,
    property::{
        Charset, EndOfLine, FinalNewline, IndentStyle, MaxLineLen, TabWidth, TrimTrailingWs,
    },
};
use memchr::memchr_iter;

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

fn check_editorconfig_properties_for_line(
    cur_line_num: usize,
    cur_line: &str,
    properties: &Properties,
) -> CheckResult {
    let TabWidth::Value(tab_width) = properties.get::<TabWidth>().unwrap_or(TabWidth::Value(4));

    let mut errors: CheckResult = vec![];
    let cur_line_width = line_space_width(cur_line, tab_width);

    // check indentation style
    let indent_style = properties
        .get::<IndentStyle>()
        .unwrap_or(IndentStyle::Spaces);
    let leading_whitespace_or_tabs_str = first_non_whitespace_or_tab_pos(cur_line)
        .map(|pos| &cur_line[0..pos])
        .unwrap_or(cur_line);
    let spaces = memchr_iter(b' ', leading_whitespace_or_tabs_str.as_bytes()).count();
    let tabs = memchr_iter(b'\t', leading_whitespace_or_tabs_str.as_bytes()).count();

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

    let TrimTrailingWs::Value(trim_trailing_ws) = properties
        .get::<TrimTrailingWs>()
        .unwrap_or(TrimTrailingWs::Value(true));
    if trim_trailing_ws
        && let Some(last_char) = cur_line.chars().next_back()
        && (last_char == ' ' || last_char == '\t')
    {
        errors.push(CheckError::TrailingWhitespace { line: cur_line_num });
    }

    if let MaxLineLen::Value(max_line_len) =
        properties.get::<MaxLineLen>().unwrap_or(MaxLineLen::Off)
    {
        let line_len = cur_line.chars().count();
        if line_len > max_line_len {
            errors.push(CheckError::LineTooLong {
                line: cur_line_num,
                actual_length: line_len,
                max_length: max_line_len,
            });
        }
    }

    errors
}

fn check_editorconfig_line_endings(
    contents: &str,
    properties: &Properties,
    empty_file_passes: bool,
) -> CheckResult {
    let mut errors: CheckResult = vec![];
    if empty_file_passes && contents.is_empty() {
        return errors;
    }
    let line_ending_mode = properties.get::<EndOfLine>().unwrap_or(EndOfLine::Lf);
    let desired_le = match line_ending_mode {
        EndOfLine::Cr => "\r",
        EndOfLine::Lf => "\n",
        EndOfLine::CrLf => "\r\n",
    };

    let desired_endings =
        memchr::memmem::find_iter(contents.as_bytes(), desired_le.as_bytes()).count();

    let crs = memchr_iter(b'\r', contents.as_bytes()).count();
    let lfs = memchr_iter(b'\n', contents.as_bytes()).count();

    let line_endings_match = match line_ending_mode {
        EndOfLine::Cr => crs == desired_endings && lfs == 0,
        EndOfLine::Lf => crs == 0 && lfs == desired_endings,
        EndOfLine::CrLf => crs == desired_endings && lfs == desired_endings,
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

    errors
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

    errors.extend_from_slice(&check_editorconfig_line_endings(
        &decoded_string,
        properties,
        true,
    ));

    for (i, line) in decoded_string.lines().enumerate() {
        errors.extend_from_slice(&check_editorconfig_properties_for_line(
            i + 1,
            line,
            properties,
        ));
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

        let errors = check_editorconfig_properties_for_line(1, "    code", &properties);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_check_indent_style_spaces() {
        let mut properties = Properties::default();
        properties.insert(IndentSize::Value(4));
        properties.insert(TabWidth::Value(4));
        properties.insert(IndentStyle::Spaces);

        let errors = check_editorconfig_properties_for_line(1, "    code", &properties);
        assert!(errors.is_empty());

        let errors = check_editorconfig_properties_for_line(1, "\tcode", &properties);
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

        let errors = check_editorconfig_properties_for_line(1, "\tcode", &properties);
        assert!(errors.is_empty());

        let errors = check_editorconfig_properties_for_line(1, "    code", &properties);
        assert_eq!(errors.len(), 1);
        match &errors[0] {
            CheckError::WrongIndentStyle { expected, .. } => {
                assert_eq!(*expected, IndentStyle::Tabs);
            }
            _ => panic!("Expected WrongIndentStyle error"),
        }
    }

    #[test]
    fn test_check_trailing_whitespace() {
        let mut properties = Properties::default();
        properties.insert(TrimTrailingWs::Value(true));
        properties.insert(IndentStyle::Spaces);

        let errors = check_editorconfig_properties_for_line(1, "code  ", &properties);
        assert_eq!(errors.len(), 1);
        match &errors[0] {
            CheckError::TrailingWhitespace { line } => {
                assert_eq!(*line, 1);
            }
            _ => panic!("Expected TrailingWhitespace error"),
        }

        let errors = check_editorconfig_properties_for_line(1, "code", &properties);
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

        let errors = check_editorconfig_properties_for_line(1, "short line", &properties);
        assert!(
            errors
                .iter()
                .all(|e| !matches!(e, CheckError::LineTooLong { .. }))
        );

        let errors =
            check_editorconfig_properties_for_line(1, "this is a very long line", &properties);
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

        let errors = check_editorconfig_line_endings("", &properties, true);
        assert!(errors.is_empty());

        let errors = check_editorconfig_line_endings("", &properties, false);
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

        let errors = check_editorconfig_line_endings("line1\nline2\n", &properties, true);
        assert!(errors.is_empty());

        let errors = check_editorconfig_line_endings("line1\r\nline2\r\n", &properties, true);
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

        let errors = check_editorconfig_line_endings("line1\r\nline2\r\n", &properties, true);
        assert!(errors.is_empty());

        let errors = check_editorconfig_line_endings("line1\nline2\n", &properties, true);
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

        let errors = check_editorconfig_line_endings("line1\rline2\r", &properties, true);
        assert!(errors.is_empty());

        let errors = check_editorconfig_line_endings("line1\nline2\n", &properties, true);
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn test_check_final_newline_present() {
        let mut properties = Properties::default();
        properties.insert(EndOfLine::Lf);
        properties.insert(FinalNewline::Value(true));

        let errors = check_editorconfig_line_endings("line1\nline2\n", &properties, true);
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

        let errors = check_editorconfig_line_endings("line1\nline2", &properties, true);
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
}
