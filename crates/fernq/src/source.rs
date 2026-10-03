//! The source model: loaded source files and positions within them.
//!
//! A [`SourceTable`] owns every file that one compilation loads. Each file
//! gets a [`SourceId`], assigned in load order and valid only for that
//! compilation. A file keeps its path as given, without canonicalization.
//! Files are immutable after loading; consumers borrow them from the table.
//!
//! A file must be UTF-8 and at most its [`SourceLimit`] long. Its text is
//! stored exactly as read: the byte order mark, CRLF line endings, and a
//! shebang line are kept for the lexer to handle. A [`ByteOffset`] is a
//! position in bytes from the start of one file, and a [`Span`] is a range of
//! them; neither identifies its file. Line and column are not part of the
//! source model.

use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::num::TryFromIntError;
use std::path::{Path, PathBuf};
use std::string::FromUtf8Error;

/// The length, in bytes, of the longest text whose offsets a [`ByteOffset`]
/// can represent. No [`SourceLimit`] exceeds it.
pub(crate) const MAX_REPRESENTABLE_SOURCE_LEN: u32 = u32::MAX;

/// The admission limit of one source file: the largest number of bytes the
/// [`SourceTable`] loads.
///
/// It is a Fernq resource policy, not a Rust rule: it bounds the memory that
/// one input can make the compiler take, about one byte per admitted byte.
/// It is at least 1 and at most [`MAX_REPRESENTABLE_SOURCE_LEN`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SourceLimit(u32);

impl SourceLimit {
    /// The limit when the command line sets none: 128 MiB.
    pub(crate) const DEFAULT: Self = Self(128 * 1024 * 1024);

    /// Returns the limit of `bytes`, or `None` when `bytes` is 0 or greater
    /// than [`MAX_REPRESENTABLE_SOURCE_LEN`].
    pub(crate) fn new(bytes: u64) -> Option<Self> {
        u32::try_from(bytes)
            .ok()
            .filter(|&bytes| bytes != 0)
            .map(Self)
    }

    /// The limit in bytes.
    pub(crate) fn bytes(self) -> u32 {
        self.0
    }
}

// A limit is a nonzero `u32`, so it never exceeds the representable length.
const _: () = assert!(
    MAX_REPRESENTABLE_SOURCE_LEN == u32::MAX && SourceLimit::DEFAULT.0 != 0,
    "every source limit lies in the representable range"
);

/// The identity of a file in the [`SourceTable`] that loaded it.
///
/// Only the table constructs an id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SourceId(u32);

/// A position in bytes from the start of one source file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ByteOffset(u32);

impl TryFrom<usize> for ByteOffset {
    type Error = TryFromIntError;

    fn try_from(offset: usize) -> Result<Self, Self::Error> {
        u32::try_from(offset).map(Self)
    }
}

impl TryFrom<ByteOffset> for usize {
    type Error = TryFromIntError;

    fn try_from(offset: ByteOffset) -> Result<Self, Self::Error> {
        usize::try_from(offset.0)
    }
}

impl fmt::Display for ByteOffset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// The half-open byte range `lo..hi` of one source file, with `lo <= hi`.
///
/// Offsets are in the coordinates of the text as stored, before any lexical
/// normalization. An empty span is a position between two bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Span {
    lo: ByteOffset,
    hi: ByteOffset,
}

impl Span {
    /// Returns the span `lo..hi`, or `None` when `lo > hi`.
    pub(crate) fn new(lo: ByteOffset, hi: ByteOffset) -> Option<Self> {
        (lo <= hi).then_some(Self { lo, hi })
    }

    /// The offset of the first byte in the span.
    pub(crate) fn lo(self) -> ByteOffset {
        self.lo
    }

    /// The offset just past the last byte in the span.
    #[cfg(any(test, feature = "fuzzing"))]
    pub(crate) fn hi(self) -> ByteOffset {
        self.hi
    }
}

/// A loaded source file. Its text is valid UTF-8 of at most
/// [`MAX_REPRESENTABLE_SOURCE_LEN`] bytes.
#[derive(Debug)]
pub(crate) struct SourceFile {
    path: PathBuf,
    text: String,
}

impl SourceFile {
    /// The path the file was loaded from, as given.
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// The text exactly as read.
    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    /// The length of the text, which is also the offset just past its end.
    pub(crate) fn len(&self) -> ByteOffset {
        ByteOffset::try_from(self.text().len()).expect(
            "the source table admits no text longer than MAX_REPRESENTABLE_SOURCE_LEN bytes",
        )
    }
}

/// The files loaded by one compilation, indexed by [`SourceId`].
#[derive(Debug, Default)]
pub(crate) struct SourceTable {
    files: Vec<SourceFile>,
}

impl SourceTable {
    /// Loads the file at `path`, of at most `limit` bytes, and returns its id.
    ///
    /// A path that names an already loaded file loads it again under a new id.
    pub(crate) fn load(&mut self, path: &Path, limit: SourceLimit) -> Result<SourceId, LoadError> {
        let text = read_file(path, limit.bytes())?;
        self.add(path.to_path_buf(), text)
    }

    /// Returns the file with id `id`, which this table must have created.
    ///
    /// The table cannot detect an id from another table: when this table has
    /// a file at the same load position, it returns that file.
    ///
    /// # Panics
    ///
    /// Panics if this table has no file with id `id`, which only an id from
    /// another table can cause.
    pub(crate) fn get(&self, id: SourceId) -> &SourceFile {
        usize::try_from(id.0)
            .ok()
            .and_then(|index| self.files.get(index))
            .expect("a SourceId indexes the table that created it")
    }

    fn add(&mut self, path: PathBuf, text: String) -> Result<SourceId, LoadError> {
        let id = u32::try_from(self.files.len())
            .map(SourceId)
            .map_err(|_| LoadError::TooManyFiles)?;
        if ByteOffset::try_from(text.len()).is_err() {
            return Err(LoadError::TooLarge {
                limit: MAX_REPRESENTABLE_SOURCE_LEN,
            });
        }
        self.files.push(SourceFile { path, text });
        Ok(id)
    }
}

#[cfg(any(test, feature = "fuzzing"))]
impl SourceTable {
    /// Adds `text` as the file at `path` without reading the file system.
    ///
    /// # Panics
    ///
    /// Panics if the table cannot admit the text: tests and the fuzz entry
    /// add only short texts to a table with few files.
    pub(crate) fn add_text(&mut self, path: &str, text: &str) -> SourceId {
        self.add(PathBuf::from(path), text.to_owned())
            .expect("a test or fuzz text fits the source table")
    }
}

/// The reason a source file cannot be loaded.
#[derive(Debug)]
pub(crate) enum LoadError {
    NotFound,
    PermissionDenied,
    Directory,
    /// Any other failure to open, inspect, or read the file.
    Io(io::Error),
    /// The file is longer than `limit` bytes.
    TooLarge {
        limit: u32,
    },
    /// The text is not UTF-8.
    NotUtf8(InvalidUtf8),
    /// The table already holds a file for every possible [`SourceId`].
    TooManyFiles,
}

impl LoadError {
    fn from_io(error: io::Error) -> Self {
        match error.kind() {
            io::ErrorKind::NotFound => Self::NotFound,
            io::ErrorKind::PermissionDenied => Self::PermissionDenied,
            _ => Self::Io(error),
        }
    }
}

/// Text that is not UTF-8, kept up to the first byte of its first invalid
/// sequence.
#[derive(Debug)]
pub(crate) struct InvalidUtf8 {
    valid: String,
    offset: ByteOffset,
}

impl InvalidUtf8 {
    /// Keeps the text of `error` before its first invalid sequence, or returns
    /// `None` when that text is too long for a [`ByteOffset`].
    pub(crate) fn new(error: FromUtf8Error) -> Option<Self> {
        let valid_len = error.utf8_error().valid_up_to();
        let mut bytes = error.into_bytes();
        bytes.truncate(valid_len);
        let valid = String::from_utf8(bytes)
            .expect("the bytes before the first invalid sequence are valid UTF-8");
        let offset = ByteOffset::try_from(valid.len()).ok()?;
        Some(Self { valid, offset })
    }

    /// The offset of the first byte of the first invalid sequence.
    pub(crate) fn offset(&self) -> ByteOffset {
        self.offset
    }

    /// The text before [`offset`](Self::offset).
    pub(crate) fn into_valid(self) -> String {
        self.valid
    }
}

/// Opens `path` and reads it as source text of at most `limit` bytes.
///
/// A directory, or a regular file whose metadata length exceeds `limit`, is
/// rejected before any read. Any other file, such as a FIFO, is read until end
/// of file or until it exceeds `limit`.
fn read_file(path: &Path, limit: u32) -> Result<String, LoadError> {
    let file = File::open(path).map_err(LoadError::from_io)?;
    let metadata = file.metadata().map_err(LoadError::from_io)?;
    if metadata.is_dir() {
        return Err(LoadError::Directory);
    }
    let mut capacity = 0;
    if metadata.is_file() {
        check_len(metadata.len(), limit)?;
        // Only a hint: the file can grow before the read, which enforces the limit.
        capacity = usize::try_from(metadata.len()).unwrap_or(0);
    }
    read_text(file, limit, capacity)
}

/// Rejects a length greater than `limit` bytes.
fn check_len(len: u64, limit: u32) -> Result<(), LoadError> {
    if len > u64::from(limit) {
        return Err(LoadError::TooLarge { limit });
    }
    Ok(())
}

/// Reads `reader` to its end as UTF-8 text of at most `limit` bytes.
///
/// Reading stops one byte past `limit`, so an unbounded reader is rejected
/// without reading more. `capacity` is the number of bytes to reserve up front.
fn read_text(reader: impl Read, limit: u32, capacity: usize) -> Result<String, LoadError> {
    let mut bytes = Vec::with_capacity(capacity);
    reader
        .take(u64::from(limit) + 1)
        .read_to_end(&mut bytes)
        .map_err(LoadError::from_io)?;
    let len = u64::try_from(bytes.len()).map_err(|_| LoadError::TooLarge { limit })?;
    check_len(len, limit)?;
    String::from_utf8(bytes).map_err(|error| {
        InvalidUtf8::new(error).map_or(LoadError::TooLarge { limit }, LoadError::NotUtf8)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(input: &[u8], limit: u32) -> Result<String, LoadError> {
        read_text(input, limit, 0)
    }

    #[test]
    fn reads_empty_input() {
        assert_eq!(read(b"", 16).unwrap(), "");
    }

    #[test]
    fn reads_valid_input() {
        assert_eq!(read(b"fn main() {}\n", 16).unwrap(), "fn main() {}\n");
    }

    #[test]
    fn reports_invalid_utf8_at_offset_zero() {
        let result = read(b"\xff\xfe", 16);
        let Err(LoadError::NotUtf8(invalid)) = result else {
            panic!("{result:?}");
        };
        assert_eq!(invalid.offset(), ByteOffset(0));
        assert_eq!(invalid.into_valid(), "");
    }

    #[test]
    fn reports_the_offset_of_the_first_invalid_byte() {
        let result = read(b"fn main() {}\n// \xff\xff", 32);
        let Err(LoadError::NotUtf8(invalid)) = result else {
            panic!("{result:?}");
        };
        assert_eq!(invalid.offset(), ByteOffset(16));
        assert_eq!(invalid.into_valid(), "fn main() {}\n// ");
    }

    #[test]
    fn reads_input_of_exactly_the_limit() {
        assert_eq!(read(b"abcd", 4).unwrap(), "abcd");
    }

    #[test]
    fn rejects_input_one_byte_over_the_limit() {
        let result = read(b"abcde", 4);
        assert!(
            matches!(result, Err(LoadError::TooLarge { limit: 4 })),
            "{result:?}"
        );
    }

    #[test]
    fn rejects_an_unbounded_reader_after_one_byte_over_the_limit() {
        let result = read_text(io::repeat(b' '), 4, 0);
        assert!(
            matches!(result, Err(LoadError::TooLarge { limit: 4 })),
            "{result:?}"
        );
    }

    #[test]
    fn a_source_limit_lies_in_the_representable_range() {
        assert_eq!(SourceLimit::new(0), None);
        assert_eq!(SourceLimit::new(1).map(SourceLimit::bytes), Some(1));
        assert_eq!(
            SourceLimit::new(u64::from(u32::MAX)).map(SourceLimit::bytes),
            Some(MAX_REPRESENTABLE_SOURCE_LEN)
        );
        assert_eq!(SourceLimit::new(u64::from(u32::MAX) + 1), None);
        assert_eq!(SourceLimit::new(u64::MAX), None);
        assert_eq!(SourceLimit::DEFAULT.bytes(), 134_217_728);
    }

    #[test]
    fn keeps_the_byte_order_mark_and_crlf() {
        let input = b"\xef\xbb\xbffn main() {}\r\n";
        assert_eq!(read(input, 32).unwrap().as_bytes(), input);
    }

    #[test]
    fn accepts_a_length_at_the_limit() {
        assert!(check_len(4, 4).is_ok());
    }

    #[test]
    fn rejects_a_length_one_over_the_limit() {
        assert!(matches!(
            check_len(5, 4),
            Err(LoadError::TooLarge { limit: 4 })
        ));
    }

    #[test]
    fn rejects_a_length_above_u32_max() {
        assert!(matches!(
            check_len(u64::from(u32::MAX) + 1, MAX_REPRESENTABLE_SOURCE_LEN),
            Err(LoadError::TooLarge {
                limit: MAX_REPRESENTABLE_SOURCE_LEN
            })
        ));
    }

    #[test]
    fn a_span_may_be_empty() {
        let span = Span::new(ByteOffset(3), ByteOffset(3)).unwrap();
        assert_eq!(
            span,
            Span {
                lo: ByteOffset(3),
                hi: ByteOffset(3)
            }
        );
    }

    #[test]
    fn a_span_keeps_its_ends() {
        let span = Span::new(ByteOffset(0), ByteOffset(u32::MAX)).unwrap();
        assert_eq!(span.lo(), ByteOffset(0));
        assert_eq!(
            span,
            Span {
                lo: ByteOffset(0),
                hi: ByteOffset(u32::MAX)
            }
        );
    }

    #[test]
    fn a_span_rejects_lo_after_hi() {
        assert_eq!(Span::new(ByteOffset(4), ByteOffset(3)), None);
    }

    #[test]
    fn added_files_get_distinct_ids() {
        let mut table = SourceTable::default();
        let a = table
            .add(PathBuf::from("a.rs"), "fn a() {}".to_owned())
            .unwrap();
        let b = table
            .add(PathBuf::from("b.rs"), "fn b() {}".to_owned())
            .unwrap();

        assert_ne!(a, b);
        assert_eq!(table.get(a).path(), Path::new("a.rs"));
        assert_eq!(table.get(a).text(), "fn a() {}");
        assert_eq!(table.get(b).path(), Path::new("b.rs"));
        assert_eq!(table.get(b).text(), "fn b() {}");
    }
}
