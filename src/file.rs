use crate::error::{CheckError, CheckResult};

use ec4rs::{
    Properties,
    property::{
        EndOfLine, FinalNewline, IndentSize, IndentStyle, MaxLineLen, TabWidth, TrimTrailingWs,
    },
};
use memchr::memchr_iter;

fn last_non_whitespace_or_tab_pos(line: &str) -> Option<usize> {
    line.bytes()
        .enumerate()
        .take_while(|(_, b)| *b == b' ' || *b == b'\t')
        .last()
        .map(|(pos, _)| pos)
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
    let leading_whitespace_or_tabs_str = last_non_whitespace_or_tab_pos(cur_line)
        .map(|pos| &cur_line[0..pos + 1])
        .unwrap_or("");
    let spaces = memchr_iter(b' ', leading_whitespace_or_tabs_str.as_bytes()).count();
    let tabs = memchr_iter(b'\t', leading_whitespace_or_tabs_str.as_bytes()).count();

    match indent_style {
        IndentStyle::Spaces => {
            if spaces != cur_line_width {
                errors.push(CheckError::WrongIndentStyle {
                    line: cur_line_num,
                    expected: indent_style,
                    expected_tabs: 0,
                    expected_spaces: cur_line_width,
                    actual_spaces: spaces,
                    actual_tabs: tabs,
                });
            }
        }
        IndentStyle::Tabs => {
            let desired_tabs = cur_line_width.div_euclid(tab_width);
            let desired_spaces = cur_line_width.rem_euclid(tab_width);

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
        }
    }

    let TrimTrailingWs::Value(trim_trailing_ws) = properties
        .get::<TrimTrailingWs>()
        .unwrap_or(TrimTrailingWs::Value(true));
    if trim_trailing_ws && cur_line.trim_end().len() != cur_line.len() {
        errors.push(CheckError::TrailingWhitespace { line: cur_line_num });
    }

    if let MaxLineLen::Value(max_line_len) =
        properties.get::<MaxLineLen>().unwrap_or(MaxLineLen::Off)
    {
        // todo: handle charsets
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

fn check_editorconfig_line_endings(contents: &str, properties: &Properties) -> CheckResult {
    let mut errors: CheckResult = vec![];
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
            expected: desired_le.escape_unicode().to_string()
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

pub fn check_file_against_editorconfig(contents: &str, properties: &Properties) -> CheckResult {
    let mut errors = vec![];
    errors.extend_from_slice(&check_editorconfig_line_endings(contents, properties));
    for (i, line) in contents.lines().enumerate() {
        errors.extend_from_slice(&check_editorconfig_properties_for_line(i, line, properties));
    }
    errors
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_last_non_whitespace_or_tab_pos() {
        assert_eq!(Some(4), last_non_whitespace_or_tab_pos("     potato"));
        assert_eq!(Some(0), last_non_whitespace_or_tab_pos("\tpotato"));
        assert_eq!(Some(2), last_non_whitespace_or_tab_pos("\t  potato"));
        assert_eq!(None, last_non_whitespace_or_tab_pos("potato"));
        assert_eq!(Some(0), last_non_whitespace_or_tab_pos(" "));
        assert_eq!(Some(1), last_non_whitespace_or_tab_pos("  "));
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
        assert!(errors.iter().all(|e| !matches!(e, CheckError::TrailingWhitespace { .. })));
    }

    #[test]
    fn test_check_max_line_length() {
        let mut properties = Properties::default();
        properties.insert(MaxLineLen::Value(10));
        properties.insert(IndentStyle::Spaces);

        let errors = check_editorconfig_properties_for_line(1, "short line", &properties);
        assert!(errors.iter().all(|e| !matches!(e, CheckError::LineTooLong { .. })));

        let errors = check_editorconfig_properties_for_line(1, "this is a very long line", &properties);
        assert_eq!(errors.iter().filter(|e| matches!(e, CheckError::LineTooLong { .. })).count(), 1);
        match errors.iter().find(|e| matches!(e, CheckError::LineTooLong { .. })) {
            Some(CheckError::LineTooLong { line, actual_length, max_length }) => {
                assert_eq!(*line, 1);
                assert_eq!(*actual_length, 24);
                assert_eq!(*max_length, 10);
            }
            _ => panic!("Expected LineTooLong error"),
        }
    }

    #[test]
    fn test_check_line_endings_lf() {
        let mut properties = Properties::default();
        properties.insert(EndOfLine::Lf);

        let errors = check_editorconfig_line_endings("line1\nline2\n", &properties);
        assert!(errors.is_empty());

        let errors = check_editorconfig_line_endings("line1\r\nline2\r\n", &properties);
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

        let errors = check_editorconfig_line_endings("line1\r\nline2\r\n", &properties);
        assert!(errors.is_empty());

        let errors = check_editorconfig_line_endings("line1\nline2\n", &properties);
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

        let errors = check_editorconfig_line_endings("line1\rline2\r", &properties);
        assert!(errors.is_empty());

        let errors = check_editorconfig_line_endings("line1\nline2\n", &properties);
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn test_check_final_newline_present() {
        let mut properties = Properties::default();
        properties.insert(EndOfLine::Lf);
        properties.insert(FinalNewline::Value(true));

        let errors = check_editorconfig_line_endings("line1\nline2\n", &properties);
        assert!(errors.iter().all(|e| !matches!(e, CheckError::MissingFinalNewline)));
    }

    #[test]
    fn test_check_final_newline_missing() {
        let mut properties = Properties::default();
        properties.insert(EndOfLine::Lf);
        properties.insert(FinalNewline::Value(true));

        let errors = check_editorconfig_line_endings("line1\nline2", &properties);
        assert_eq!(errors.iter().filter(|e| matches!(e, CheckError::MissingFinalNewline)).count(), 1);
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
        let errors = check_file_against_editorconfig("  line1\n  line2\n", &properties);
        assert!(errors.is_empty());

        // File with multiple errors
        let errors = check_file_against_editorconfig("   line1  \n\tline2", &properties);
        assert!(errors.len() > 1);
    }
}
