//! A calculation's name: its file's name inside the folder.

use std::fmt;

use serde::{Deserialize, Serialize};

/// What every calculation's file name ends with.
pub(crate) const EXTENSION: &str = ".txt";

/// The longest file name most file systems take, in bytes.
pub(crate) const MAX_NAME_BYTES: usize = 255;

/// A calculation's file name inside the folder, such as `Trip to Rome.txt`.
///
/// It is always a plain, visible `.txt` name: never a path, never hidden,
/// so no id read from disk or from the metadata can name a file outside the
/// folder.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct CalculationId(String);

impl CalculationId {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The name without its `.txt`.
    pub fn stem(&self) -> &str {
        // Valid ids end with the ASCII `.txt`, so this is a char boundary.
        &self.0[..self.0.len() - EXTENSION.len()]
    }

    /// The name as a case-insensitive file system compares it.
    pub(crate) fn folded(&self) -> String {
        fold(&self.0)
    }

    /// Whether `name` is a calculation's file name.
    pub(crate) fn is_valid(name: &str) -> bool {
        !name.is_empty()
            && name.len() <= MAX_NAME_BYTES
            && !name.starts_with('.')
            && !name.contains(['/', '\\', '\0'])
            && name.len() > EXTENSION.len()
            && name
                .get(name.len() - EXTENSION.len()..)
                .is_some_and(|end| end.eq_ignore_ascii_case(EXTENSION))
    }
}

/// `name` as a case-insensitive file system (macOS's, by default) compares
/// it.
pub(crate) fn fold(name: &str) -> String {
    name.to_lowercase()
}

impl TryFrom<String> for CalculationId {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if Self::is_valid(&value) {
            Ok(Self(value))
        } else {
            Err(format!("not a calculation's file name: {value:?}"))
        }
    }
}

impl TryFrom<&str> for CalculationId {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::try_from(value.to_owned())
    }
}

impl fmt::Display for CalculationId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}
