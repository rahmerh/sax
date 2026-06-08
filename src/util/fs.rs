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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveType {
    Zip,
    Tar,
    TarGz,
    TarXz,
    TarBz2,
    TarZst,
    SevenZip,
    Rar,
}

const ARCHIVE_SUFFIXES: [(&str, ArchiveType); 12] = [
    ("tar.bz2", ArchiveType::TarBz2),
    ("tar.zst", ArchiveType::TarZst),
    ("tar.gz", ArchiveType::TarGz),
    ("tar.xz", ArchiveType::TarXz),
    ("tbz2", ArchiveType::TarBz2),
    ("tzst", ArchiveType::TarZst),
    ("zip", ArchiveType::Zip),
    ("tar", ArchiveType::Tar),
    ("tgz", ArchiveType::TarGz),
    ("txz", ArchiveType::TarXz),
    ("7z", ArchiveType::SevenZip),
    ("rar", ArchiveType::Rar),
];

#[derive(Debug, Error)]
#[error("unsupported archive type: {path}")]
pub struct DetectArchiveTypeError {
    path: PathBuf,
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

pub fn detect_archive_type(path: &Path) -> Result<ArchiveType, DetectArchiveTypeError> {
    let lowercase_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_lowercase();

    ARCHIVE_SUFFIXES
        .iter()
        .find(|(suffix, _)| lowercase_name.ends_with(suffix))
        .map(|(_, archive_type)| *archive_type)
        .ok_or_else(|| DetectArchiveTypeError {
            path: path.to_path_buf(),
        })
}

pub fn is_archive(path: &Path) -> bool {
    detect_archive_type(path).is_ok()
}

pub fn archive_stem(path: &Path) -> Option<&str> {
    let name = path.file_name()?.to_str()?;
    let lowercase_name = name.to_lowercase();
    let (suffix, _) = ARCHIVE_SUFFIXES
        .iter()
        .find(|(suffix, _)| lowercase_name.ends_with(suffix))?;
    let stem = name.get(..name.len() - suffix.len())?.strip_suffix('.')?;

    (!stem.is_empty()).then_some(stem)
}

#[cfg(test)]
mod tests {
    use kernal::prelude::*;
    use tempfile::TempDir;

    use super::*;
    use std::io::Cursor;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn detect_archive_type_should_assert_correct_archive_type_enum() {
        // Arrange
        let archive_types = [
            ("test.zip", ArchiveType::Zip),
            ("test.tar", ArchiveType::Tar),
            ("test.tar.gz", ArchiveType::TarGz),
            ("test.tgz", ArchiveType::TarGz),
            ("test.tar.xz", ArchiveType::TarXz),
            ("test.txz", ArchiveType::TarXz),
            ("test.tar.bz2", ArchiveType::TarBz2),
            ("test.tbz2", ArchiveType::TarBz2),
            ("test.tar.zst", ArchiveType::TarZst),
            ("test.tzst", ArchiveType::TarZst),
            ("test.7z", ArchiveType::SevenZip),
            ("test.rar", ArchiveType::Rar),
        ];

        // Assert
        for (name, expected) in archive_types {
            let actual = detect_archive_type(Path::new(name)).unwrap();

            assert_that!(actual).is_equal_to(expected);
        }
    }

    #[test]
    fn is_archive_should_return_true_when_correct_archive_name_given() {
        // Arrange
        let path = Path::new("a.tar.gz");

        // Act
        let actual = is_archive(path);

        // Assert
        assert_that!(actual).is_true();
    }

    #[test]
    fn is_archive_should_return_false_when_incorrect_archive_name_given() {
        // Arrange
        let path = Path::new("foo");

        // Act
        let actual = is_archive(path);

        // Assert
        assert_that!(actual).is_false();
    }

    #[test]
    fn archive_stem_should_strip_full_archive_extension() {
        for (archive, expected) in [
            ("test.zip", "test"),
            ("test.tar", "test"),
            ("test.tar.gz", "test"),
            ("test.tgz", "test"),
            ("test.tar.xz", "test"),
            ("test.txz", "test"),
            ("test.tar.bz2", "test"),
            ("test.tbz2", "test"),
            ("test.tar.zst", "test"),
            ("test.tzst", "test"),
            ("test.7z", "test"),
            ("test.rar", "test"),
        ] {
            assert_that!(archive_stem(Path::new(archive))).is_equal_to(Some(expected));
        }
    }

    #[test]
    fn archive_stem_should_only_return_the_basename() {
        assert_that!(archive_stem(Path::new("/tmp/my.archive.tar.gz")))
            .is_equal_to(Some("my.archive"));
    }

    #[test]
    fn archive_stem_should_handle_uppercase_extensions() {
        assert_that!(archive_stem(Path::new("archive.TAR.GZ"))).is_equal_to(Some("archive"));
    }

    #[test]
    fn archive_stem_should_return_none_for_unsupported_or_empty_names() {
        assert_that!(archive_stem(Path::new("archive.txt"))).is_none();
        assert_that!(archive_stem(Path::new(".zip"))).is_none();
    }

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
        assert_that!(out.join("root.txt")).is_file_with_content("hello");
        assert_that!(out.join("nested/inner.txt")).is_file_with_content("world");
        assert_that!(source).is_empty_dir();
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
        assert_that!(out.join("root.txt")).is_file_with_content("new file");
        assert_that!(out.join("nested/inner.txt")).is_file_with_content("new nested");
        assert_that!(out.join("nested/old.txt")).does_not_exist();
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
        assert_that!(path).does_not_exist();
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
        assert_that!(path).does_not_exist();
    }

    #[test]
    fn remove_existing_path_should_ignore_missing_path() {
        // Arrange
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("missing");

        // Act
        let actual = remove_existing_path(&path);

        // Assert
        assert_that!(actual).is_ok();
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
        assert_that!(path).is_file_with_content("hello");
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
        assert_that!(fs::metadata(path).unwrap().permissions().mode() & 0o777).is_equal_to(0o600);
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
        assert_that!(fs::metadata(path).unwrap().permissions().mode() & 0o777).is_equal_to(0o755);
    }

    #[test]
    fn safe_join_should_return_unsafe_path_error_when_empty_path_given() {
        // Arrange
        let base = PathBuf::from("/tmp");
        let empty = PathBuf::from("");

        // Act
        let actual = safe_join(&base, &empty);

        // Assert
        assert_that!(matches!(actual.unwrap_err(), UnsafePathError::Empty)).is_true();
    }

    #[test]
    fn safe_join_should_return_unsafe_path_error_when_absolute_path_given() {
        // Arrange
        let base = PathBuf::from("/tmp");
        let empty = PathBuf::from("/absolute");

        // Act
        let actual = safe_join(&base, &empty);

        // Assert
        assert_that!(matches!(actual.unwrap_err(), UnsafePathError::Absolute)).is_true();
    }

    #[test]
    fn safe_join_should_return_unsafe_path_error_when_nested_traversel_path_given() {
        // Arrange
        let base = PathBuf::from("/tmp");
        let empty = PathBuf::from("nested/../../escape");

        // Act
        let actual = safe_join(&base, &empty);

        // Assert
        assert_that!(matches!(actual.unwrap_err(), UnsafePathError::Traversal)).is_true();
    }

    #[test]
    fn safe_join_should_join_two_paths() {
        // Arrange
        let base = PathBuf::from("/tmp");
        let empty = PathBuf::from("foo/bar");

        // Act
        let actual = safe_join(&base, &empty).unwrap();

        // Assert
        assert_that!(actual).is_equal_to(PathBuf::from("/tmp/foo/bar"));
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
        assert_that!(actual).is_equal_to(child);
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
        assert_that!(actual).is_equal_to(dir.path().to_path_buf());
    }

    #[test]
    fn single_child_dir_or_self_should_return_self_for_single_file() {
        // Arrange
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("README.txt"), "hello").unwrap();

        // Act
        let actual = single_child_dir_or_self(dir.path()).unwrap();

        // Assert
        assert_that!(actual).is_equal_to(dir.path().to_path_buf());
    }
}
