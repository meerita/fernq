//! The `fernq` command-line grammar.
//!
//! The parser maps the arguments after the program name to a [`Command`] or a
//! [`UsageError`]. It performs no I/O and does not inspect the paths it
//! returns. Writing output and choosing the exit status belong to `main`.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use crate::edition::Edition;
use crate::source::SourceLimit;

/// The synopsis of a valid invocation.
pub(crate) const USAGE: &str =
    "usage: fernq <INPUT> -o <OUTPUT> --edition <EDITION> [--max-input-bytes <BYTES>]";

/// A command line that the grammar accepts.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Command {
    /// `-h` or `--help` as the only argument.
    Help,
    /// One input path, one output path, one edition, and the input size limit.
    Compile(Invocation),
}

/// The configuration of a valid invocation: two non-empty paths, the edition
/// of the input, and the admission limit of the input.
///
/// Only [`parse`] constructs an invocation.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Invocation {
    input: PathBuf,
    output: PathBuf,
    edition: Edition,
    input_limit: SourceLimit,
}

impl Invocation {
    pub(crate) fn input(&self) -> &Path {
        &self.input
    }

    pub(crate) fn output(&self) -> &Path {
        &self.output
    }

    pub(crate) fn edition(&self) -> Edition {
        self.edition
    }

    /// The admission limit of the input: `--max-input-bytes`, or
    /// [`SourceLimit::DEFAULT`] when the option is absent.
    pub(crate) fn input_limit(&self) -> SourceLimit {
        self.input_limit
    }
}

/// The grammar rule that a command line violates.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum UsageError {
    NoArguments,
    /// A token that starts with `-` and is not `-o`, `--edition`,
    /// `--max-input-bytes`, `-h`, or `--help`.
    UnknownOption(OsString),
    /// `-o` is the last token.
    MissingOutputValue,
    DuplicateOutput,
    EmptyOutput,
    EmptyInput,
    /// An input path after the first one.
    ExtraInput(OsString),
    /// `--edition` is the last token.
    MissingEditionValue,
    DuplicateEdition,
    /// The value after `--edition` is not exactly one of the edition names.
    UnknownEdition(OsString),
    MissingInput,
    MissingOutput,
    MissingEdition,
    /// `--max-input-bytes` is the last token.
    MissingInputLimitValue,
    DuplicateInputLimit,
    /// The value after `--max-input-bytes` is not a decimal number of bytes
    /// from 1 to 4,294,967,295.
    InvalidInputLimit(OsString),
    /// `-h` or `--help` together with other arguments.
    HelpWithArguments,
}

/// Parses the arguments that follow the program name.
///
/// `<INPUT>`, `-o <OUTPUT>`, `--edition <EDITION>`, and the optional
/// `--max-input-bytes <BYTES>` may appear in any order. The token after `-o`
/// is the output path verbatim, even when it starts with `-`; the token after
/// `--edition` or `--max-input-bytes` is its value in the same way.
/// Options match only as exact tokens, so `-oFILE`, `-o=FILE`,
/// `--edition=2024`, `-`, and `--` are unknown options. Paths need not be
/// valid UTF-8.
pub(crate) fn parse(args: Vec<OsString>) -> Result<Command, UsageError> {
    match args.as_slice() {
        [] => return Err(UsageError::NoArguments),
        [only] if is_help(only) => return Ok(Command::Help),
        _ => {}
    }

    let mut input = None;
    let mut output = None;
    let mut edition = None;
    let mut input_limit = None;
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
        } else if arg == "--edition" {
            if edition.is_some() {
                return Err(UsageError::DuplicateEdition);
            }
            let Some(value) = args.next() else {
                return Err(UsageError::MissingEditionValue);
            };
            let Some(value) = value.to_str().and_then(Edition::from_name) else {
                return Err(UsageError::UnknownEdition(value));
            };
            edition = Some(value);
        } else if arg == "--max-input-bytes" {
            if input_limit.is_some() {
                return Err(UsageError::DuplicateInputLimit);
            }
            let Some(value) = args.next() else {
                return Err(UsageError::MissingInputLimitValue);
            };
            let Some(limit) = parse_input_limit(&value) else {
                return Err(UsageError::InvalidInputLimit(value));
            };
            input_limit = Some(limit);
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
    let edition = edition.ok_or(UsageError::MissingEdition)?;
    Ok(Command::Compile(Invocation {
        input,
        output,
        edition,
        input_limit: input_limit.unwrap_or(SourceLimit::DEFAULT),
    }))
}

/// Returns the limit that `value` names: decimal digits only, from 1 to
/// 4,294,967,295. Leading zeros are digits like any other.
fn parse_input_limit(value: &OsStr) -> Option<SourceLimit> {
    let digits = value.to_str()?;
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    // A value too long for `u64` is out of range as well.
    SourceLimit::new(digits.parse().ok()?)
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
        compile_edition(input, output, Edition::E2024)
    }

    fn compile_edition(input: &str, output: &str, edition: Edition) -> Result<Command, UsageError> {
        Ok(Command::Compile(Invocation {
            input: PathBuf::from(input),
            output: PathBuf::from(output),
            edition,
            input_limit: SourceLimit::DEFAULT,
        }))
    }

    fn compile_limit(bytes: u64) -> Result<Command, UsageError> {
        Ok(Command::Compile(Invocation {
            input: PathBuf::from("main.rs"),
            output: PathBuf::from("main"),
            edition: Edition::E2024,
            input_limit: SourceLimit::new(bytes).unwrap(),
        }))
    }

    #[test]
    fn accepts_input_before_output() {
        assert_eq!(
            parse_strs(&["main.rs", "-o", "main", "--edition", "2024"]),
            compile("main.rs", "main")
        );
    }

    #[test]
    fn accepts_output_before_input() {
        assert_eq!(
            parse_strs(&["-o", "main", "main.rs", "--edition", "2024"]),
            compile("main.rs", "main")
        );
    }

    #[test]
    fn accepts_each_edition_before_and_after_the_paths() {
        for (name, edition) in [
            ("2015", Edition::E2015),
            ("2018", Edition::E2018),
            ("2021", Edition::E2021),
            ("2024", Edition::E2024),
        ] {
            let expected = compile_edition("main.rs", "main", edition);
            assert_eq!(
                parse_strs(&["--edition", name, "main.rs", "-o", "main"]),
                expected
            );
            assert_eq!(
                parse_strs(&["main.rs", "--edition", name, "-o", "main"]),
                expected
            );
            assert_eq!(
                parse_strs(&["main.rs", "-o", "main", "--edition", name]),
                expected
            );
        }
    }

    #[test]
    fn the_input_limit_is_the_default_without_the_option() {
        let Ok(Command::Compile(invocation)) =
            parse_strs(&["main.rs", "-o", "main", "--edition", "2024"])
        else {
            panic!("the command line is valid");
        };
        assert_eq!(invocation.input_limit(), SourceLimit::DEFAULT);
    }

    #[test]
    fn accepts_an_input_limit_in_any_position() {
        for (value, bytes) in [
            ("1", 1),
            ("134217728", 134_217_728),
            ("4294967295", 4_294_967_295),
            ("0042", 42),
        ] {
            let expected = compile_limit(bytes);
            for args in [
                [
                    "--max-input-bytes",
                    value,
                    "main.rs",
                    "-o",
                    "main",
                    "--edition",
                    "2024",
                ],
                [
                    "main.rs",
                    "--max-input-bytes",
                    value,
                    "-o",
                    "main",
                    "--edition",
                    "2024",
                ],
                [
                    "main.rs",
                    "-o",
                    "main",
                    "--edition",
                    "2024",
                    "--max-input-bytes",
                    value,
                ],
            ] {
                assert_eq!(parse_strs(&args), expected, "{args:?}");
            }
        }
    }

    #[test]
    fn rejects_an_invalid_input_limit_and_keeps_its_value() {
        for value in [
            "",
            "0",
            "4294967296",
            "99999999999999999999999",
            "-1",
            "+1",
            " 1",
            "1 ",
            "1e3",
            "0x10",
            "128MiB",
            "-o",
        ] {
            assert_eq!(
                parse_strs(&["main.rs", "-o", "main", "--max-input-bytes", value]),
                Err(UsageError::InvalidInputLimit(OsString::from(value))),
                "{value:?}"
            );
        }
    }

    #[test]
    fn rejects_input_limit_as_the_last_token() {
        assert_eq!(
            parse_strs(&["main.rs", "-o", "main", "--max-input-bytes"]),
            Err(UsageError::MissingInputLimitValue)
        );
    }

    #[test]
    fn rejects_input_limit_twice() {
        assert_eq!(
            parse_strs(&[
                "main.rs",
                "--max-input-bytes",
                "1",
                "--max-input-bytes",
                "2"
            ]),
            Err(UsageError::DuplicateInputLimit)
        );
    }

    #[test]
    fn rejects_an_attached_input_limit_value_as_an_unknown_option() {
        assert!(matches!(
            parse_strs(&["main.rs", "-o", "main", "--max-input-bytes=4"]),
            Err(UsageError::UnknownOption(_))
        ));
    }

    #[test]
    fn accepts_both_help_spellings_alone() {
        assert_eq!(parse_strs(&["-h"]), Ok(Command::Help));
        assert_eq!(parse_strs(&["--help"]), Ok(Command::Help));
    }

    #[test]
    fn takes_the_token_after_o_verbatim() {
        assert_eq!(
            parse_strs(&["main.rs", "-o", "-out", "--edition", "2024"]),
            compile("main.rs", "-out")
        );
        assert_eq!(
            parse_strs(&["main.rs", "-o", "-h", "--edition", "2024"]),
            compile("main.rs", "-h")
        );
        assert_eq!(
            parse_strs(&["main.rs", "-o", "--", "--edition", "2024"]),
            compile("main.rs", "--")
        );
        assert_eq!(
            parse_strs(&["main.rs", "-o", "--edition", "--edition", "2024"]),
            compile("main.rs", "--edition")
        );
    }

    #[test]
    fn takes_the_token_after_edition_as_its_value() {
        assert!(matches!(
            parse_strs(&["main.rs", "-o", "main", "--edition", "-o"]),
            Err(UsageError::UnknownEdition(value)) if value == "-o"
        ));
    }

    #[test]
    fn rejects_a_missing_edition() {
        assert_eq!(
            parse_strs(&["main.rs", "-o", "main"]),
            Err(UsageError::MissingEdition)
        );
    }

    #[test]
    fn rejects_edition_as_the_last_token() {
        assert_eq!(
            parse_strs(&["main.rs", "-o", "main", "--edition"]),
            Err(UsageError::MissingEditionValue)
        );
    }

    #[test]
    fn rejects_edition_twice() {
        assert_eq!(
            parse_strs(&[
                "main.rs",
                "-o",
                "main",
                "--edition",
                "2021",
                "--edition",
                "2024"
            ]),
            Err(UsageError::DuplicateEdition)
        );
        assert_eq!(
            parse_strs(&["main.rs", "--edition", "2024", "--edition"]),
            Err(UsageError::DuplicateEdition)
        );
    }

    #[test]
    fn rejects_an_unknown_edition_and_keeps_its_value() {
        for value in ["2027", "future", "", " 2024"] {
            assert_eq!(
                parse_strs(&["main.rs", "-o", "main", "--edition", value]),
                Err(UsageError::UnknownEdition(OsString::from(value)))
            );
        }
    }

    #[test]
    fn rejects_an_attached_edition_value_as_an_unknown_option() {
        assert!(matches!(
            parse_strs(&["main.rs", "-o", "main", "--edition=2024"]),
            Err(UsageError::UnknownOption(_))
        ));
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
            parse_strs(&["main.rs", "-o", "main", "--verbose"]),
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
            parse(vec![
                input.clone(),
                OsString::from("-o"),
                output.clone(),
                OsString::from("--edition"),
                OsString::from("2024"),
            ]),
            Ok(Command::Compile(Invocation {
                input: PathBuf::from(input),
                output: PathBuf::from(output),
                edition: Edition::E2024,
                input_limit: SourceLimit::DEFAULT,
            }))
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_a_non_utf8_edition_value_and_keeps_it() {
        use std::os::unix::ffi::OsStringExt;

        let value = OsString::from_vec(b"20\xff24".to_vec());
        assert_eq!(
            parse(vec![
                OsString::from("main.rs"),
                OsString::from("-o"),
                OsString::from("main"),
                OsString::from("--edition"),
                value.clone(),
            ]),
            Err(UsageError::UnknownEdition(value))
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
