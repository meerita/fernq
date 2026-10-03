//! The source model: loaded source files and positions within them.
//!
//! A [`SourceTable`] owns every file that one compilation loads. Each file
//! gets a [`SourceId`], assigned in load order and valid only for that
//! compilation. A file keeps its path as given, without canonicalization.
//! Files are immutable after loading; consumers borrow them from the table.
//!
//! A file must be UTF-8 and at most [`MAX_SOURCE_LEN`] bytes long. Its text is
//! stored exactly as read: the byte order mark, CRLF line endings, and a
//! shebang line are kept for the lexer to handle. A [`ByteOffset`] is a
//! position in bytes from the start of one file; line and column are not part
//! of the source model.

use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::num::TryFromIntError;
use std::path::{Path, PathBuf};
use std::string::FromUtf8Error;

/// The maximum length of one source file, in bytes.
pub(crate) const MAX_SOURCE_LEN: u32 = u32::MAX;

/// The identity of a file in the [`SourceTable`] that loaded it.
///
/// Only the table constructs an id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SourceId(u32);

/// A position in bytes from the start of one source file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ByteOffset(u32);

impl TryFrom<usize> for ByteOffset {
    type Error = TryFromIntError;

    fn try_from(offset: usize) -> Result<Self, Self::Error> {
        u32::try_from(offset).map(Self)
    }
}

impl fmt::Display for ByteOffset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// A loaded source file. Its text is valid UTF-8 of at most
/// [`MAX_SOURCE_LEN`] bytes.
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
        ByteOffset::try_from(self.text().len())
            .expect("the source table admits no text longer than MAX_SOURCE_LEN bytes")
    }
}

/// The files loaded by one compilation, indexed by [`SourceId`].
#[derive(Debug, Default)]
pub(crate) struct SourceTable {
    files: Vec<SourceFile>,
}

impl SourceTable {
    /// Loads the file at `path` and returns its id.
    ///
    /// A path that names an already loaded file loads it again under a new id.
    pub(crate) fn load(&mut self, path: &Path) -> Result<SourceId, LoadError> {
        let text = read_file(path, MAX_SOURCE_LEN)?;
        self.add(path.to_path_buf(), text)
    }

    /// Returns the file with id `id`.
    ///
    /// # Panics
    ///
    /// Panics if `id` was created by another table.
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
                limit: MAX_SOURCE_LEN,
            });
        }
        self.files.push(SourceFile { path, text });
        Ok(id)
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
            check_len(u64::from(u32::MAX) + 1, MAX_SOURCE_LEN),
            Err(LoadError::TooLarge {
                limit: MAX_SOURCE_LEN
            })
        ));
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
