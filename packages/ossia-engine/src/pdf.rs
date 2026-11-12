use lopdf::encryption::crypt_filters::{Aes128CryptFilter, CryptFilter};
use lopdf::{Document, EncryptionState, EncryptionVersion, Permissions};
use std::{collections::BTreeMap, sync::Arc};

use crate::*;

/// Encrypt a PDF with an open password while preserving normal viewing and printing.
pub fn protect_pdf(bytes: &[u8], password: &str) -> Result<Vec<u8>, AppError> {
    let mut document = Document::load_mem(bytes).map_err(|error| {
        failure(
            "pdf-encrypt",
            &format!("could not read generated PDF: {error}"),
        )
    })?;
    let crypt_filter: Arc<dyn CryptFilter> = Arc::new(Aes128CryptFilter);
    let version = EncryptionVersion::V4 {
        document: &document,
        encrypt_metadata: true,
        crypt_filters: BTreeMap::from([(b"StdCF".to_vec(), crypt_filter)]),
        stream_filter: b"StdCF".to_vec(),
        string_filter: b"StdCF".to_vec(),
        owner_password: password,
        user_password: password,
        permissions: Permissions::default(),
    };
    let state = EncryptionState::try_from(version).map_err(|error| {
        failure(
            "pdf-encrypt",
            &format!("could not prepare PDF encryption: {error}"),
        )
    })?;
    document.encrypt(&state).map_err(|error| {
        failure(
            "pdf-encrypt",
            &format!("could not encrypt generated PDF: {error}"),
        )
    })?;
    let mut output = Vec::new();
    document.save_to(&mut output).map_err(|error| {
        failure(
            "pdf-encrypt",
            &format!("could not write encrypted PDF: {error}"),
        )
    })?;
    Ok(output)
}

#[cfg(test)]
#[path = "../tests/unit/pdf.rs"]
mod tests;
