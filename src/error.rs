use ec4rs::property::{Charset, IndentStyle};

#[derive(Debug, Clone, PartialEq)]
pub enum CheckError {
    WrongIndentStyleBasic {
        line: usize,
        expected: IndentStyle,
        actual_tabs: usize,
        actual_spaces: usize,
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
    WrongFileEncoding {
        expected: Charset,
        actual: Charset,
    },
    IncorrectFileEncoding {
        charset: Charset,
    },
    IOError,
    Skipped,
}

impl std::fmt::Display for CheckError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CheckError::WrongIndentStyleBasic {
                line,
                expected,
                actual_tabs,
                actual_spaces,
            } => write!(
                f,
                "line {}: wrong indent style (expected {:?}: got {} tabs, {} spaces)",
                line, expected, actual_tabs, actual_spaces
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
            CheckError::WrongFileEncoding { expected, actual } => {
                write!(
                    f,
                    "file expected to be encoded as {}, but sniffed {}",
                    expected, actual
                )
            }
            CheckError::IncorrectFileEncoding { charset } => {
                write!(
                    f,
                    "unable to decode file as {}, replacements were applied",
                    charset
                )
            }
            CheckError::IOError => {
                write!(f, "unable to open or read file")
            }
            CheckError::Skipped => Ok(()),
        }
    }
}

pub type CheckResult = Vec<CheckError>;
