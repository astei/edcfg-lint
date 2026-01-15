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

fn line_space_width(line: &str, properties: &Properties) -> usize {
    let TabWidth::Value(tab_width) = properties.get::<TabWidth>().unwrap_or(TabWidth::Value(4));

    // skip over consecutive runs of `tabs_or_spaces` positions, on the theory the first non-whitespace/tab character will
    // occur after a run of positions
    line.bytes()
        .take_while(|&b| b == b' ' || b == b'\t')
        .map(|b| if b == b'\t' { tab_width } else { 1 })
        .sum()
}

fn check_editorconfig_properties_for_line(cur_line: &str, properties: &Properties) -> bool {
    let cur_line_width = line_space_width(cur_line, properties);

    let TabWidth::Value(tab_width) = properties.get::<TabWidth>().unwrap_or(TabWidth::Value(4));
    let indent_size_raw = properties
        .get::<IndentSize>()
        .unwrap_or(IndentSize::Value(4));
    let indent_size = match indent_size_raw {
        IndentSize::Value(spec_indent_size) => spec_indent_size,
        IndentSize::UseTabWidth => tab_width,
    };

    // todo: add error reporting

    // check if line width matches indent size
    if cur_line_width.rem(indent_size) != 0 {
        return false;
    }

    // check indentation style
    let indent_style = properties.get::<IndentStyle>().unwrap_or(IndentStyle::Tabs);
    let leading_whitespace_or_tabs_str = last_non_whitespace_or_tab_pos(cur_line)
        .map(|pos| &cur_line[0..pos + 1])
        .unwrap_or("");
    let spaces = leading_whitespace_or_tabs_str
        .bytes()
        .filter(|&b| b == b' ')
        .count();
    match indent_style {
        IndentStyle::Spaces => {
            if spaces != cur_line_width {
                return false;
            }
        }
        IndentStyle::Tabs => {
            let tabs = leading_whitespace_or_tabs_str
                .bytes()
                .filter(|&b| b == b'\t')
                .count();
            let desired_tabs = cur_line_width.div_euclid(tab_width);
            let desired_spaces = cur_line_width.rem_euclid(tab_width);

            if desired_tabs != tabs || spaces != desired_spaces {
                return false;
            }
        }
    }

    let TrimTrailingWs::Value(trim_trailing_ws) = properties
        .get::<TrimTrailingWs>()
        .unwrap_or(TrimTrailingWs::Value(true));
    if trim_trailing_ws && cur_line.trim_end().len() != cur_line.len() {
        return false;
    }

    if let MaxLineLen::Value(max_line_len) =
        properties.get::<MaxLineLen>().unwrap_or(MaxLineLen::Off)
    {
        // todo: handle charsets
        if cur_line.chars().count() > max_line_len {
            return false;
        }
    }

    true
}

fn check_editorconfig_line_endings(contents: &str, properties: &Properties) -> bool {
    let line_ending_mode = properties.get::<EndOfLine>().unwrap_or(EndOfLine::CrLf);
    let desired_le = match line_ending_mode {
        EndOfLine::Cr => "\r",
        EndOfLine::Lf => "\n",
        EndOfLine::CrLf => "\r\n",
    };

    let desired_endings = memchr::memmem::find_iter(desired_le.as_bytes(), contents).count();

    let crs = memrchr_iter(b'\r', contents.as_bytes()).count();
    let lfs = memrchr_iter(b'\n', contents.as_bytes()).count();

    let line_endings_match = match line_ending_mode {
        EndOfLine::Cr => crs == desired_endings && lfs == 0,
        EndOfLine::Lf => crs == 0 && lfs == desired_endings,
        EndOfLine::CrLf => crs == desired_endings && lfs == desired_endings,
    };

    if !line_endings_match {
        return false;
    }

    if let Some(FinalNewline::Value(final_newline)) = properties.get::<FinalNewline>().ok()
        && final_newline
        && &contents[contents.len() - 2..] != desired_le {
            return false;
        }

    true
}

pub fn check_file_against_editorconfig(contents: &str, properties: &Properties) -> bool {
    if check_editorconfig_line_endings(contents, properties) {
        return false;
    }

    contents
        .lines()
        .all(|l| check_editorconfig_properties_for_line(l, properties))
}
