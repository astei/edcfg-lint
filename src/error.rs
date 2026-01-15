use ec4rs::property::IndentStyle;

#[derive(Debug, Clone, PartialEq)]
pub enum CheckError {
    InvalidIndentSize {
        line: usize,
        actual_width: usize,
        indent_size: usize,
    },
    WrongIndentStyle {
        line: usize,
        expected: IndentStyle,
        expected_tabs: usize,
        expected_spaces: usize,
        actual_tabs: usize,
        actual_spaces: usize,
    },
    TrailingWhitespace {
        line: usize,
    },
    LineTooLong {
        line: usize,
        actual_length: usize,
        max_length: usize,
    },
    WrongLineEnding {
        expected: String,
    },
    MissingFinalNewline,
}

impl std::fmt::Display for CheckError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CheckError::InvalidIndentSize {
                line,
                actual_width,
                indent_size,
            } => write!(
                f,
                "line {}: invalid indent size (width {} is not a multiple of {})",
                line, actual_width, indent_size
            ),
            CheckError::WrongIndentStyle {
                line,
                expected,
                expected_tabs,
                expected_spaces,
                actual_tabs,
                actual_spaces,
            } => write!(
                f,
                "line {}: wrong indent style (expected {:?}: {} tabs, {} spaces; got {} tabs, {} spaces)",
                line, expected, expected_tabs, expected_spaces, actual_tabs, actual_spaces
            ),
            CheckError::TrailingWhitespace { line } => {
                write!(f, "line {}: trailing whitespace", line)
            }
            CheckError::LineTooLong {
                line,
                actual_length,
                max_length,
            } => write!(
                f,
                "line {}: line too long ({} > {} characters)",
                line, actual_length, max_length
            ),
            CheckError::WrongLineEnding { expected } => {
                write!(f, "wrong line ending (expected {:?})", expected)
            }
            CheckError::MissingFinalNewline => {
                write!(f, "missing final newline")
            }
        }
    }
}

pub type CheckResult = Vec<CheckError>;
