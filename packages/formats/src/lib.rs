pub mod animclip;
pub mod bc7_enc;
pub mod bxml;
pub mod cube;
pub mod emb;
pub mod erp;
pub mod gvdb;
pub mod hash;
pub mod lng;
pub mod lz4;
pub mod material;
pub mod nefs;
pub mod pe;
pub mod pkg;
pub mod surface;
pub mod texarray;
pub mod texture;
pub mod uipre;

mod bytes;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("unexpected end of data at {0:#x}")]
    Eof(usize),
    #[error("bad magic at {offset:#x}: expected {expected}")]
    Magic { offset: usize, expected: &'static str },
    #[error("{0}")]
    Format(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("{}: {}", path.display(), io_text(source))]
    IoAt {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// A fragment marked as zstd whose bytes do not decode, with the archive file and the resource it was read
    /// from when it came from one.
    #[error(
        "{}fragment {fragment}: its {packed} byte(s) are marked as zstd but do not decode ({reason}), so the archive is damaged or was written wrong",
        frame_at(.path.as_deref(), .resource.as_deref())
    )]
    Frame {
        path: Option<std::path::PathBuf>,
        resource: Option<String>,
        fragment: String,
        packed: usize,
        reason: String,
    },
}

fn frame_at(path: Option<&std::path::Path>, resource: Option<&str>) -> String {
    let path = path.map(|p| p.display().to_string());
    path.into_iter()
        .chain(resource.map(str::to_owned))
        .map(|part| format!("{part}: "))
        .collect()
}

pub type Result<T> = std::result::Result<T, Error>;

fn io_text(e: &std::io::Error) -> String {
    match e.kind() {
        std::io::ErrorKind::UnexpectedEof => {
            "the file is shorter than it should be, so it is incomplete or damaged".into()
        }
        _ => e.to_string(),
    }
}

/// An I/O error on `path`, named in the message.
pub(crate) fn io_at(path: &std::path::Path) -> impl FnOnce(std::io::Error) -> Error + '_ {
    move |source| Error::IoAt {
        path: path.to_path_buf(),
        source,
    }
}
