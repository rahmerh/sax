use std::fs;
use std::io::{self, Read};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum UnsafePathError {
    #[error("empty paths are not allowed")]
    Empty,
    #[error("absolute paths are not allowed")]
    Absolute,
    #[error("path traversal ('..') is not allowed")]
    Traversal,
}

pub fn move_dir_contents(source_dir: &Path, out: &Path) -> io::Result<()> {
    for entry in fs::read_dir(source_dir)? {
        let entry = entry?;
        let source_path = entry.path();
        let target_path = out.join(entry.file_name());

        remove_existing_path(&target_path)?;
        fs::rename(source_path, target_path)?;
    }

    Ok(())
}

pub fn single_child_dir_or_self(path: &Path) -> io::Result<PathBuf> {
    let entries = fs::read_dir(path)?.collect::<Result<Vec<_>, _>>()?;

    if entries.len() == 1 {
        let only_entry_path = entries[0].path();

        if only_entry_path.is_dir() {
            return Ok(only_entry_path);
        }
    }

    Ok(path.to_path_buf())
}

pub fn remove_existing_path(path: &Path) -> io::Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err),
    };

    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

pub fn safe_join(base: &Path, rel: &Path) -> Result<PathBuf, UnsafePathError> {
    if rel.as_os_str().is_empty() {
        return Err(UnsafePathError::Empty);
    }

    if rel.is_absolute() {
        return Err(UnsafePathError::Absolute);
    }

    let mut clean = PathBuf::new();
    for component in rel.components() {
        match component {
            Component::Normal(part) => clean.push(part),
            Component::CurDir => {}
            Component::ParentDir => return Err(UnsafePathError::Traversal),
            Component::RootDir | Component::Prefix(_) => {
                return Err(UnsafePathError::Absolute);
            }
        }
    }

    if clean.as_os_str().is_empty() {
        return Err(UnsafePathError::Empty);
    }

    Ok(base.join(clean))
}

pub fn write_file_from_reader(path: &Path, mode: u32, reader: &mut dyn Read) -> io::Result<()> {
    let mut file = fs::File::create(path)?;
    io::copy(reader, &mut file)?;
    set_permissions(path, mode)?;
    Ok(())
}

pub fn set_permissions(path: &Path, mode: u32) -> io::Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(mode & 0o777))
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use std::io::Cursor;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn move_dir_contents_should_move_files_and_directories_into_output() {
        // Arrange
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source");
        let out = dir.path().join("out");
        let nested = source.join("nested");
        fs::create_dir_all(&nested).unwrap();
        fs::create_dir(&out).unwrap();
        fs::write(source.join("root.txt"), "hello").unwrap();
        fs::write(nested.join("inner.txt"), "world").unwrap();

        // Act
        move_dir_contents(&source, &out).unwrap();

        // Assert
        assert_eq!(fs::read_to_string(out.join("root.txt")).unwrap(), "hello");
        assert_eq!(
            fs::read_to_string(out.join("nested/inner.txt")).unwrap(),
            "world"
        );
        assert!(fs::read_dir(&source).unwrap().next().is_none());
    }

    #[test]
    fn move_dir_contents_should_overwrite_existing_targets() {
        // Arrange
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source");
        let out = dir.path().join("out");
        fs::create_dir_all(source.join("nested")).unwrap();
        fs::create_dir_all(out.join("nested")).unwrap();
        fs::write(source.join("root.txt"), "new file").unwrap();
        fs::write(source.join("nested/inner.txt"), "new nested").unwrap();
        fs::write(out.join("root.txt"), "old file").unwrap();
        fs::write(out.join("nested/old.txt"), "old nested").unwrap();

        // Act
        move_dir_contents(&source, &out).unwrap();

        // Assert
        assert_eq!(
            fs::read_to_string(out.join("root.txt")).unwrap(),
            "new file"
        );
        assert_eq!(
            fs::read_to_string(out.join("nested/inner.txt")).unwrap(),
            "new nested"
        );
        assert!(!out.join("nested/old.txt").exists());
    }

    #[test]
    fn remove_existing_path_should_remove_file() {
        // Arrange
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("file.txt");
        fs::write(&path, "hello").unwrap();

        // Act
        remove_existing_path(&path).unwrap();

        // Assert
        assert!(!path.exists());
    }

    #[test]
    fn remove_existing_path_should_remove_directory() {
        // Arrange
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nested");
        fs::create_dir(&path).unwrap();
        fs::write(path.join("inner.txt"), "hello").unwrap();

        // Act
        remove_existing_path(&path).unwrap();

        // Assert
        assert!(!path.exists());
    }

    #[test]
    fn remove_existing_path_should_ignore_missing_path() {
        // Arrange
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("missing");

        // Act
        let actual = remove_existing_path(&path);

        // Assert
        assert!(actual.is_ok());
    }

    #[test]
    fn write_file_from_reader_should_write_contents() {
        // Arrange
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("file.txt");
        let mut input = Cursor::new("hello");

        // Act
        write_file_from_reader(&path, 0o644, &mut input).unwrap();

        // Assert
        assert_eq!(fs::read_to_string(path).unwrap(), "hello");
    }

    #[cfg(unix)]
    #[test]
    fn write_file_from_reader_should_set_permissions() {
        // Arrange
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("file.txt");
        let mut input = Cursor::new("hello");

        // Act
        write_file_from_reader(&path, 0o600, &mut input).unwrap();

        // Assert
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[cfg(unix)]
    #[test]
    fn set_permissions_should_mask_to_permission_bits() {
        // Arrange
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("file.txt");
        fs::write(&path, "hello").unwrap();

        // Act
        set_permissions(&path, 0o100755).unwrap();

        // Assert
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o755
        );
    }

    #[test]
    fn safe_join_should_return_unsafe_path_error_when_empty_path_given() {
        // Arrange
        let base = PathBuf::from("/tmp");
        let empty = PathBuf::from("");

        // Act
        let actual = safe_join(&base, &empty);

        // Assert
        assert!(actual.is_err());
        assert!(matches!(actual.err().unwrap(), UnsafePathError::Empty))
    }

    #[test]
    fn safe_join_should_return_unsafe_path_error_when_absolute_path_given() {
        // Arrange
        let base = PathBuf::from("/tmp");
        let empty = PathBuf::from("/absolute");

        // Act
        let actual = safe_join(&base, &empty);

        // Assert
        assert!(actual.is_err());
        assert!(matches!(actual.err().unwrap(), UnsafePathError::Absolute))
    }

    #[test]
    fn safe_join_should_return_unsafe_path_error_when_nested_traversel_path_given() {
        // Arrange
        let base = PathBuf::from("/tmp");
        let empty = PathBuf::from("nested/../../escape");

        // Act
        let actual = safe_join(&base, &empty);

        // Assert
        assert!(actual.is_err());
        assert!(matches!(actual.err().unwrap(), UnsafePathError::Traversal));
    }

    #[test]
    fn safe_join_should_join_two_paths() {
        // Arrange
        let base = PathBuf::from("/tmp");
        let empty = PathBuf::from("foo/bar");

        // Act
        let actual = safe_join(&base, &empty).unwrap();

        // Assert
        assert_eq!(actual, PathBuf::from("/tmp/foo/bar"))
    }

    #[test]
    fn single_child_dir_or_self_should_return_only_child_directory() {
        // Arrange
        let dir = TempDir::new().unwrap();
        let child = dir.path().join("package");
        fs::create_dir(&child).unwrap();

        // Act
        let actual = single_child_dir_or_self(dir.path()).unwrap();

        // Assert
        assert_eq!(actual, child);
    }

    #[test]
    fn single_child_dir_or_self_should_return_self_for_multiple_entries() {
        // Arrange
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join("package")).unwrap();
        fs::write(dir.path().join("README.txt"), "hello").unwrap();

        // Act
        let actual = single_child_dir_or_self(dir.path()).unwrap();

        // Assert
        assert_eq!(actual, dir.path());
    }

    #[test]
    fn single_child_dir_or_self_should_return_self_for_single_file() {
        // Arrange
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("README.txt"), "hello").unwrap();

        // Act
        let actual = single_child_dir_or_self(dir.path()).unwrap();

        // Assert
        assert_eq!(actual, dir.path());
    }
}
