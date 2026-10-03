//! The `fernq` command-line grammar.
//!
//! The parser maps the arguments after the program name to a [`Command`] or a
//! [`UsageError`]. It performs no I/O and does not inspect the paths it
//! returns. Writing output and choosing the exit status belong to `main`.

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::path::{Path, PathBuf};

/// The synopsis of a valid invocation.
pub(crate) const USAGE: &str = "usage: fernq <INPUT> -o <OUTPUT>";

/// A command line that the grammar accepts.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Command {
    /// `-h` or `--help` as the only argument.
    Help,
    /// One input path and one output path.
    Compile(Invocation),
}

/// The paths of a valid invocation, both non-empty.
///
/// Only [`parse`] constructs an invocation.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Invocation {
    input: PathBuf,
    output: PathBuf,
}

impl Invocation {
    pub(crate) fn input(&self) -> &Path {
        &self.input
    }

    pub(crate) fn output(&self) -> &Path {
        &self.output
    }
}

/// The grammar rule that a command line violates.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum UsageError {
    NoArguments,
    /// A token that starts with `-` and is not `-o`, `-h`, or `--help`.
    UnknownOption(OsString),
    /// `-o` is the last token.
    MissingOutputValue,
    DuplicateOutput,
    EmptyOutput,
    EmptyInput,
    /// An input path after the first one.
    ExtraInput(OsString),
    MissingInput,
    MissingOutput,
    /// `-h` or `--help` together with other arguments.
    HelpWithArguments,
}

impl fmt::Display for UsageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoArguments => f.write_str("no arguments"),
            Self::UnknownOption(token) => write!(f, "unknown option '{}'", token.display()),
            Self::MissingOutputValue => f.write_str("option -o requires an output path"),
            Self::DuplicateOutput => f.write_str("option -o is given more than once"),
            Self::EmptyOutput => f.write_str("the output path is empty"),
            Self::EmptyInput => f.write_str("the input path is empty"),
            Self::ExtraInput(token) => write!(
                f,
                "unexpected argument '{}'; only one input path is accepted",
                token.display()
            ),
            Self::MissingInput => f.write_str("missing input path"),
            Self::MissingOutput => f.write_str("missing output path; pass -o <OUTPUT>"),
            Self::HelpWithArguments => f.write_str("-h and --help take no other arguments"),
        }
    }
}

/// Parses the arguments that follow the program name.
///
/// `<INPUT>` and `-o <OUTPUT>` may appear in either order. The token after
/// `-o` is the output path verbatim, even when it starts with `-`. Options
/// match only as exact tokens, so `-oFILE`, `-o=FILE`, `-`, and `--` are
/// unknown options. Paths need not be valid UTF-8.
pub(crate) fn parse(args: Vec<OsString>) -> Result<Command, UsageError> {
    match args.as_slice() {
        [] => return Err(UsageError::NoArguments),
        [only] if is_help(only) => return Ok(Command::Help),
        _ => {}
    }

    let mut input = None;
    let mut output = None;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if is_help(&arg) {
            return Err(UsageError::HelpWithArguments);
        }
        if arg == "-o" {
            if output.is_some() {
                return Err(UsageError::DuplicateOutput);
            }
            let Some(path) = args.next() else {
                return Err(UsageError::MissingOutputValue);
            };
            if path.is_empty() {
                return Err(UsageError::EmptyOutput);
            }
            output = Some(PathBuf::from(path));
        } else if starts_with_dash(&arg) {
            return Err(UsageError::UnknownOption(arg));
        } else if input.is_some() {
            return Err(UsageError::ExtraInput(arg));
        } else if arg.is_empty() {
            return Err(UsageError::EmptyInput);
        } else {
            input = Some(PathBuf::from(arg));
        }
    }

    let input = input.ok_or(UsageError::MissingInput)?;
    let output = output.ok_or(UsageError::MissingOutput)?;
    Ok(Command::Compile(Invocation { input, output }))
}

fn is_help(arg: &OsStr) -> bool {
    arg == "-h" || arg == "--help"
}

// Compare encoded bytes so that a non-UTF-8 token never needs decoding.
fn starts_with_dash(arg: &OsStr) -> bool {
    arg.as_encoded_bytes().first() == Some(&b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_strs(args: &[&str]) -> Result<Command, UsageError> {
        parse(args.iter().map(OsString::from).collect())
    }

    fn compile(input: &str, output: &str) -> Result<Command, UsageError> {
        Ok(Command::Compile(Invocation {
            input: PathBuf::from(input),
            output: PathBuf::from(output),
        }))
    }

    #[test]
    fn accepts_input_before_output() {
        assert_eq!(
            parse_strs(&["main.rs", "-o", "main"]),
            compile("main.rs", "main")
        );
    }

    #[test]
    fn accepts_output_before_input() {
        assert_eq!(
            parse_strs(&["-o", "main", "main.rs"]),
            compile("main.rs", "main")
        );
    }

    #[test]
    fn accepts_both_help_spellings_alone() {
        assert_eq!(parse_strs(&["-h"]), Ok(Command::Help));
        assert_eq!(parse_strs(&["--help"]), Ok(Command::Help));
    }

    #[test]
    fn takes_the_token_after_o_verbatim() {
        assert_eq!(
            parse_strs(&["main.rs", "-o", "-out"]),
            compile("main.rs", "-out")
        );
        assert_eq!(
            parse_strs(&["main.rs", "-o", "-h"]),
            compile("main.rs", "-h")
        );
        assert_eq!(
            parse_strs(&["main.rs", "-o", "--"]),
            compile("main.rs", "--")
        );
    }

    #[test]
    fn rejects_no_arguments() {
        assert_eq!(parse_strs(&[]), Err(UsageError::NoArguments));
    }

    #[test]
    fn rejects_a_single_dash_as_an_unknown_option() {
        assert!(matches!(
            parse_strs(&["-", "-o", "main"]),
            Err(UsageError::UnknownOption(_))
        ));
    }

    #[test]
    fn rejects_a_double_dash_as_an_unknown_option() {
        assert!(matches!(
            parse_strs(&["--", "main.rs", "-o", "main"]),
            Err(UsageError::UnknownOption(_))
        ));
    }

    #[test]
    fn rejects_attached_output_values_as_unknown_options() {
        assert!(matches!(
            parse_strs(&["main.rs", "-omain"]),
            Err(UsageError::UnknownOption(_))
        ));
        assert!(matches!(
            parse_strs(&["main.rs", "-o=main"]),
            Err(UsageError::UnknownOption(_))
        ));
    }

    #[test]
    fn rejects_an_unknown_option() {
        assert!(matches!(
            parse_strs(&["main.rs", "-o", "main", "--edition"]),
            Err(UsageError::UnknownOption(_))
        ));
    }

    #[test]
    fn rejects_an_empty_output_path() {
        assert_eq!(
            parse_strs(&["main.rs", "-o", ""]),
            Err(UsageError::EmptyOutput)
        );
    }

    #[test]
    fn rejects_an_empty_input_path() {
        assert_eq!(parse_strs(&["", "-o", "main"]), Err(UsageError::EmptyInput));
    }

    #[test]
    fn rejects_o_as_the_last_token() {
        assert_eq!(
            parse_strs(&["main.rs", "-o"]),
            Err(UsageError::MissingOutputValue)
        );
    }

    #[test]
    fn rejects_o_twice() {
        assert_eq!(
            parse_strs(&["main.rs", "-o", "a", "-o", "b"]),
            Err(UsageError::DuplicateOutput)
        );
    }

    #[test]
    fn rejects_o_twice_even_when_the_second_has_no_value() {
        assert_eq!(
            parse_strs(&["main.rs", "-o", "a", "-o"]),
            Err(UsageError::DuplicateOutput)
        );
    }

    #[test]
    fn rejects_two_inputs() {
        assert!(matches!(
            parse_strs(&["a.rs", "b.rs", "-o", "main"]),
            Err(UsageError::ExtraInput(_))
        ));
    }

    #[test]
    fn rejects_input_without_output() {
        assert_eq!(parse_strs(&["main.rs"]), Err(UsageError::MissingOutput));
    }

    #[test]
    fn rejects_output_without_input() {
        assert_eq!(parse_strs(&["-o", "main"]), Err(UsageError::MissingInput));
    }

    #[test]
    fn rejects_help_with_other_arguments() {
        assert_eq!(
            parse_strs(&["-h", "main.rs"]),
            Err(UsageError::HelpWithArguments)
        );
        assert_eq!(
            parse_strs(&["main.rs", "-o", "main", "--help"]),
            Err(UsageError::HelpWithArguments)
        );
        assert_eq!(
            parse_strs(&["-h", "--help"]),
            Err(UsageError::HelpWithArguments)
        );
    }

    #[cfg(unix)]
    #[test]
    fn accepts_non_utf8_paths() {
        use std::os::unix::ffi::OsStringExt;

        let input = OsString::from_vec(b"in\xffput.rs".to_vec());
        let output = OsString::from_vec(b"out\xffput".to_vec());
        assert_eq!(
            parse(vec![input.clone(), OsString::from("-o"), output.clone()]),
            Ok(Command::Compile(Invocation {
                input: PathBuf::from(input),
                output: PathBuf::from(output),
            }))
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_a_non_utf8_option_like_token_as_an_unknown_option() {
        use std::os::unix::ffi::OsStringExt;

        let token = OsString::from_vec(b"-\xff".to_vec());
        assert!(matches!(
            parse(vec![
                OsString::from("main.rs"),
                token,
                OsString::from("-o"),
                OsString::from("main"),
            ]),
            Err(UsageError::UnknownOption(_))
        ));
    }
}
