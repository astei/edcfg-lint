use crate::error::CheckError;

use std::ops::Rem;

use ec4rs::{
    Properties,
    property::{
        EndOfLine, FinalNewline, IndentSize, IndentStyle, MaxLineLen, TabWidth, TrimTrailingWs,
    },
};
use memchr::memrchr_iter;

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
) -> Vec<CheckError> {
    let TabWidth::Value(tab_width) = properties.get::<TabWidth>().unwrap_or(TabWidth::Value(4));

    let mut errors: Vec<CheckError> = vec![];
    let cur_line_width = line_space_width(cur_line, tab_width);

    let indent_size_raw = properties
        .get::<IndentSize>()
        .unwrap_or(IndentSize::Value(4));
    let indent_size = match indent_size_raw {
        IndentSize::Value(spec_indent_size) => spec_indent_size,
        IndentSize::UseTabWidth => tab_width,
    };

    // check if line width matches indent size
    if cur_line_width.rem(indent_size) != 0 {
        errors.push(CheckError::InvalidIndentSize {
            line: cur_line_num,
            actual_width: cur_line_width,
            indent_size,
        });
    }

    // check indentation style
    let indent_style = properties
        .get::<IndentStyle>()
        .unwrap_or(IndentStyle::Spaces);
    let leading_whitespace_or_tabs_str = last_non_whitespace_or_tab_pos(cur_line)
        .map(|pos| &cur_line[0..pos + 1])
        .unwrap_or("");
    let spaces = memrchr_iter(b' ', leading_whitespace_or_tabs_str.as_bytes()).count();
    let tabs = memrchr_iter(b'\t', leading_whitespace_or_tabs_str.as_bytes()).count();

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

fn check_editorconfig_line_endings(contents: &str, properties: &Properties) -> Vec<CheckError> {
    let mut errors: Vec<CheckError> = vec![];
    let line_ending_mode = properties.get::<EndOfLine>().unwrap_or(EndOfLine::Lf);
    let desired_le = match line_ending_mode {
        EndOfLine::Cr => "\r",
        EndOfLine::Lf => "\n",
        EndOfLine::CrLf => "\r\n",
    };

    let desired_endings =
        memchr::memmem::find_iter(contents.as_bytes(), desired_le.as_bytes()).count();

    let crs = memrchr_iter(b'\r', contents.as_bytes()).count();
    let lfs = memrchr_iter(b'\n', contents.as_bytes()).count();

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

pub fn check_file_against_editorconfig(contents: &str, properties: &Properties) -> Vec<CheckError> {
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
    fn test_line_space_width() {
        assert_eq!(5, line_space_width("     potato", 4));
        assert_eq!(4, line_space_width("\tpotato", 4));
        assert_eq!(6, line_space_width("\t  potato", 4));
    }
}
