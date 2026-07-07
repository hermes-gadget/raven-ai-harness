//! Sandbox — validates file paths against [`PathBoundary`] rules.
//!
//! The [`Sandbox`] ensures that file operations performed by tools stay
//! within the allowed directories and do not access denied paths.

use std::path::{Path, PathBuf};

use odin_core::error::{OdinError, OdinResult};
use odin_core::types::PathBoundary;

/// Filesystem boundary enforcer.
///
/// Wraps a [`PathBoundary`] and provides methods to check whether a given
/// path is allowed for reading or writing.
#[derive(Debug, Clone)]
pub struct Sandbox {
    boundary: PathBoundary,
}

impl Sandbox {
    /// Create a new sandbox from a [`PathBoundary`].
    pub fn new(boundary: PathBoundary) -> Self {
        Self { boundary }
    }

    /// Borrow the underlying boundary configuration.
    pub fn boundary(&self) -> &PathBoundary {
        &self.boundary
    }

    /// Check whether `path` is allowed for reading.
    ///
    /// Returns the canonicalised path on success, or an error if the path
    /// is outside the allowed boundaries or falls in the denied list.
    pub fn check_read(&self, path: &Path) -> OdinResult<PathBuf> {
        let canonical = self.resolve(path)?;
        self.check_allowed(&canonical, false)
    }

    /// Check whether `path` is allowed for writing.
    ///
    /// For paths that don't exist yet, the parent directory is used for
    /// boundary checking.
    pub fn check_write(&self, path: &Path) -> OdinResult<PathBuf> {
        let canonical = self.resolve(path)?;
        self.check_allowed(&canonical, true)
    }

    /// Try to resolve a path to its canonical (absolute) form.
    ///
    /// If the path does not exist yet, the parent chain is resolved instead
    /// and the final component is appended.
    fn resolve(&self, path: &Path) -> OdinResult<PathBuf> {
        if path.exists() {
            return path.canonicalize().map_err(OdinError::Io);
        }

        // Path doesn't exist — try resolving the parent chain
        if let Some(parent) = path.parent()
            && parent.exists()
        {
            let mut canonical = parent.canonicalize().map_err(OdinError::Io)?;
            if let Some(filename) = path.file_name() {
                canonical.push(filename);
            }
            return Ok(canonical);
        }

        // Fall back to an absolute path constructed from working-dir assumptions
        // If the path is relative, we can't resolve it without a base directory.
        // Return it as-is and let the caller handle it.
        if path.is_absolute() {
            Ok(path.to_path_buf())
        } else {
            Err(OdinError::Validation(format!(
                "Cannot resolve path '{}': does not exist and no parent exists to anchor it",
                path.display()
            )))
        }
    }

    /// Check whether a canonical path is within the allowed boundaries.
    fn check_allowed(&self, path: &Path, write: bool) -> OdinResult<PathBuf> {
        let path_str = path.to_string_lossy();

        // Check denied list first
        for denied in &self.boundary.denied {
            if path_str.starts_with(denied) || path_str == *denied {
                return Err(OdinError::PermissionDenied(format!(
                    "Path '{}' is denied by rule '{}'",
                    path.display(),
                    denied,
                )));
            }
        }

        // Check allowed list
        let allowed_list = if write {
            &self.boundary.allowed_write
        } else {
            &self.boundary.allowed_read
        };

        for allowed in allowed_list {
            let allowed_path = Path::new(allowed);
            // If the allowed path is relative, treat it as relative to cwd
            let allowed_canonical = if allowed_path.is_relative() {
                std::env::current_dir()
                    .ok()
                    .map(|cwd| cwd.join(allowed_path))
                    .unwrap_or_else(|| allowed_path.to_path_buf())
            } else {
                allowed_path.to_path_buf()
            };

            let allowed_str = allowed_canonical.to_string_lossy();
            if path_str.starts_with(allowed_str.as_ref()) || path_str == allowed_str.as_ref() {
                return Ok(path.to_path_buf());
            }
        }

        Err(OdinError::PermissionDenied(format!(
            "Path '{}' is not within allowed {} boundaries",
            path.display(),
            if write { "write" } else { "read" },
        )))
    }
}

impl Default for Sandbox {
    fn default() -> Self {
        Self::new(PathBoundary::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_default_sandbox() {
        let sandbox = Sandbox::default();
        assert!(!sandbox.boundary().allowed_read.is_empty());
    }

    #[test]
    fn test_read_allowed_in_temp() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "hello").unwrap();

        let boundary = PathBoundary {
            allowed_read: vec![dir.path().to_string_lossy().to_string()],
            allowed_write: vec![dir.path().to_string_lossy().to_string()],
            denied: vec![],
        };
        let sandbox = Sandbox::new(boundary);
        let result = sandbox.check_read(&file_path);
        assert!(result.is_ok(), "{:?}", result.err());
    }

    #[test]
    fn test_write_denied_outside_boundary() {
        let boundary = PathBoundary {
            allowed_read: vec!["/tmp".into()],
            allowed_write: vec!["/tmp".into()],
            denied: vec![],
        };
        let sandbox = Sandbox::new(boundary);
        // /etc is not in allowed_write
        let result = sandbox.check_write(Path::new("/etc/passwd"));
        assert!(result.is_err());
    }

    #[test]
    fn test_denied_path() {
        let boundary = PathBoundary {
            allowed_read: vec!["/".into()],
            allowed_write: vec!["/tmp".into()],
            denied: vec!["/etc/shadow".into()],
        };
        let sandbox = Sandbox::new(boundary);
        let result = sandbox.check_read(Path::new("/etc/shadow"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("denied"));
    }

    #[test]
    fn test_nonexistent_path_write() {
        let dir = tempfile::tempdir().unwrap();
        let new_file = dir.path().join("new_file.txt");
        // File doesn't exist yet
        assert!(!new_file.exists());

        let boundary = PathBoundary {
            allowed_read: vec![dir.path().to_string_lossy().to_string()],
            allowed_write: vec![dir.path().to_string_lossy().to_string()],
            denied: vec![],
        };
        let sandbox = Sandbox::new(boundary);
        let result = sandbox.check_write(&new_file);
        assert!(result.is_ok(), "{:?}", result.err());
    }
}
