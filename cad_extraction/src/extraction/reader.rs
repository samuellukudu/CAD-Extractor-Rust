use std::path::{Path, PathBuf};

use acadrust::{CadDocument, DxfReader, DwgReader};

use super::error::ExtractionError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CadFileFormat {
    Dxf,
    Dwg,
}

impl CadFileFormat {
    pub fn from_path(path: &Path) -> Result<Self, ExtractionError> {
        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase());

        match extension.as_deref() {
            Some("dxf") => Ok(Self::Dxf),
            Some("dwg") => Ok(Self::Dwg),
            _ => Err(ExtractionError::UnsupportedFormat(path.to_path_buf())),
        }
    }
}

pub fn read_document(path: &Path) -> Result<CadDocument, ExtractionError> {
    let normalized_path: PathBuf = path.to_path_buf();
    let format = CadFileFormat::from_path(path)?;

    match format {
        CadFileFormat::Dxf => DxfReader::from_file(path)
            .map_err(|source| ExtractionError::Parse {
                path: normalized_path.clone(),
                source,
            })?
            .read()
            .map_err(|source| ExtractionError::Parse {
                path: normalized_path,
                source,
            }),
        CadFileFormat::Dwg => {
            let mut reader = DwgReader::from_file(path).map_err(|source| ExtractionError::Parse {
                path: normalized_path.clone(),
                source,
            })?;
            reader.read().map_err(|source| ExtractionError::Parse {
                path: normalized_path,
                source,
            })
        }
    }
}
