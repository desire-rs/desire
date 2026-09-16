//! Parsed `multipart/form-data` payloads.

use bytes::Bytes;

/// One uploaded file from a multipart form.
#[derive(Debug, Clone)]
pub struct UploadedFile {
  /// The form field name.
  pub name: String,
  /// The client-provided filename, if the part declared one.
  pub filename: Option<String>,
  /// The part's content type, if declared.
  pub content_type: Option<String>,
  /// The file content (buffered; the whole request is subject to the
  /// body size limit).
  pub bytes: Bytes,
}

/// A parsed `multipart/form-data` body: text fields plus files.
#[derive(Debug, Default)]
pub struct FormData {
  pub(crate) fields: Vec<(String, String)>,
  pub(crate) files: Vec<UploadedFile>,
}

impl FormData {
  /// One text field's value.
  pub fn field(&self, name: &str) -> Option<&str> {
    self
      .fields
      .iter()
      .find(|(n, _)| n == name)
      .map(|(_, v)| v.as_str())
  }

  /// All text fields, in order of appearance.
  pub fn fields(&self) -> &[(String, String)] {
    &self.fields
  }

  /// The first file uploaded under `name`.
  pub fn file(&self, name: &str) -> Option<&UploadedFile> {
    self.files.iter().find(|f| f.name == name)
  }

  /// All files (multi-file inputs appear repeatedly under one name).
  pub fn files(&self) -> &[UploadedFile] {
    &self.files
  }
}
