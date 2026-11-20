use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DecodeError {
    #[error("the stream ends at byte {at}: {need} more byte(s) needed")]
    Truncated { at: usize, need: usize },
    #[error("unsupported stream version {found} at byte {at}; the game reads versions up to 1")]
    UnsupportedVersion { at: usize, found: u64 },
    #[error("unknown value type {id:#010x} ({name}) at byte {at}")]
    UnknownType { at: usize, id: u32, name: String },
    #[error("entry count {count} at byte {at} exceeds what the stream can hold")]
    CountTooLarge { at: usize, count: u64 },
    #[error("nesting deeper than {max} at byte {at}")]
    TooDeep { at: usize, max: usize },
    #[error("more than {max} values in the stream")]
    TooManyNodes { max: usize },
    #[error("{what} at byte {at} has a form the game never writes")]
    NonCanonical { at: usize, what: &'static str },
    #[error("{len} unread byte(s) after the bag, starting at byte {at}")]
    TrailingBytes { at: usize, len: usize },
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum EncodeError {
    #[error("{what} holds {len} items; the format counts at most {max}")]
    TooMany {
        what: &'static str,
        len: usize,
        max: usize,
    },
    #[error("a {what} of {len} byte(s) does not fit its {max}-byte length")]
    TooLong {
        what: &'static str,
        len: usize,
        max: usize,
    },
    #[error("nesting deeper than {max}")]
    TooDeep { max: usize },
    #[error("a dbrf value holds {len} ids; the format counts at most {max}")]
    TooManyIds { len: usize, max: usize },
    #[error("stream version {found} is not one the game reads")]
    UnsupportedVersion { found: u64 },
}

#[derive(Debug, Error)]
pub enum JsonError {
    #[error(transparent)]
    Decode(#[from] DecodeError),
    #[error(transparent)]
    Encode(#[from] EncodeError),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}
