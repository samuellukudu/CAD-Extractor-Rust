use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum ExtractionError {
    #[error("unsupported file extension for {0:?}; expected .dwg or .dxf")]
    UnsupportedFormat(PathBuf),
    #[error("failed to parse CAD file {path:?}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: acadrust::DxfError,
    },
    #[error("failed to open CAD file {path:?}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}
