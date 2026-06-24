use std::fs;
use std::io::{self};
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
    (".zip", ArchiveType::Zip),
    (".tar", ArchiveType::Tar),
    (".tar.gz", ArchiveType::TarGz),
    (".tgz", ArchiveType::TarGz),
    (".tar.zst", ArchiveType::TarZst),
    (".tzst", ArchiveType::TarZst),
    (".tar.xz", ArchiveType::TarXz),
    (".txz", ArchiveType::TarXz),
    (".tbz2", ArchiveType::TarBz2),
    (".tar.bz2", ArchiveType::TarBz2),
    (".7z", ArchiveType::SevenZip),
    (".rar", ArchiveType::Rar),
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

        if !target_path.parent().unwrap().exists() {
            fs::create_dir_all(target_path.parent().unwrap())?;
        }

        fs::rename(&source_path, &target_path)?;
    }

    Ok(())
}

pub fn try_get_only_child_dir(path: &Path) -> io::Result<Option<PathBuf>> {
    let mut entries = fs::read_dir(path)?;

    let Some(entry) = entries.next().transpose()? else {
        return Ok(None);
    };

    if entries.next().transpose()?.is_some() {
        return Ok(None);
    }

    let entry_path = entry.path();

    Ok(entry_path.is_dir().then_some(entry_path))
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
        .find(|(suffix, _)| {
            lowercase_name
                .strip_suffix(suffix)
                .is_some_and(|stem| !stem.is_empty())
        })
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
    let stem = name.get(..name.len() - suffix.len())?;

    (!stem.is_empty()).then_some(stem)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    use kernal::prelude::*;
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn move_dir_contents_should_move_content_recursively_when_called() {
        // Arrange
        let temp_dir = TempDir::new().unwrap();

        let input_dir = temp_dir.path().join("in");
        let nested_dir = input_dir.join("nested");
        fs::create_dir_all(&nested_dir).unwrap();
        fs::write(nested_dir.join("file.txt"), "foo").unwrap();

        let output_dir = temp_dir.path().join("out");
        fs::create_dir_all(&output_dir).unwrap();

        // Act
        let actual = move_dir_contents(&input_dir, &output_dir);

        // Assert
        assert_that!(actual).is_ok();

        let expected_file = output_dir.join("nested").join("file.txt");

        assert_that!(expected_file).exists();
    }

    #[test]
    fn move_dir_contents_should_create_all_output_dirs_when_they_dont_exist() {
        // Arrange
        let temp_dir = TempDir::new().unwrap();

        let input_dir = temp_dir.path().join("in");
        let nested_dir = input_dir.join("nested");
        fs::create_dir_all(&nested_dir).unwrap();
        fs::write(nested_dir.join("file.txt"), "foo").unwrap();

        let output_dir = temp_dir.path().join("out");

        // Act
        let actual = move_dir_contents(&input_dir, &output_dir);

        // Assert
        assert_that!(actual).is_ok();

        let expected_file = output_dir.join("nested").join("file.txt");

        assert_that!(expected_file).exists();
    }

    #[test]
    fn try_get_only_child_dir_should_return_none_when_no_dirs_in_input() {
        // Arrange
        let temp_dir = TempDir::new().unwrap();

        // Act
        let actual = try_get_only_child_dir(temp_dir.path());

        // Assert
        assert_that!(&actual).is_ok();

        let actual_value = actual.unwrap();

        assert_that!(&actual_value).is_none();
    }

    #[test]
    fn try_get_only_child_dir_should_return_none_when_more_than_one_dirs_in_input() {
        // Arrange
        let temp_dir = TempDir::new().unwrap();

        fs::create_dir_all(temp_dir.path().join("dir1")).unwrap();
        fs::create_dir_all(temp_dir.path().join("dir2")).unwrap();

        // Act
        let actual = try_get_only_child_dir(temp_dir.path());

        // Assert
        assert_that!(&actual).is_ok();

        let actual_value = actual.unwrap();

        assert_that!(&actual_value).is_none();
    }

    #[test]
    fn try_get_only_child_dir_should_return_none_when_only_entry_is_a_file() {
        // Arrange
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("test.txt"), "content").unwrap();

        // Act
        let actual = try_get_only_child_dir(temp_dir.path());

        // Assert
        assert_that!(&actual).is_ok();
        assert_that!(&actual.unwrap()).is_none();
    }

    #[test]
    fn try_get_only_child_dir_should_return_single_entry_path_when_only_one_dir_in_input() {
        // Arrange
        let temp_dir = TempDir::new().unwrap();

        let single_entry = temp_dir.path().join("dir");
        fs::create_dir_all(&single_entry).unwrap();

        // Act
        let actual = try_get_only_child_dir(temp_dir.path());

        // Assert
        assert_that!(&actual).is_ok();

        let actual_value = actual.unwrap();

        assert_that!(&actual_value).is_some();

        let actual_path = actual_value.unwrap();

        assert_that!(&actual_path).is_equal_to(&single_entry);
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
    fn safe_join_should_join_two_paths_when_relative_path_given() {
        // Arrange
        let base = PathBuf::from("/tmp");
        let empty = PathBuf::from("foo/bar");

        // Act
        let actual = safe_join(&base, &empty).unwrap();

        // Assert
        assert_that!(actual).is_equal_to(PathBuf::from("/tmp/foo/bar"));
    }

    #[test]
    fn write_file_from_reader_should_write_contents_of_reader_to_file() {
        // Arrange
        let temp_dir = TempDir::new().unwrap();

        let output_path = temp_dir.path().join("output.txt");
        let mut reader = Cursor::new("file contents");

        // Act
        let actual = write_file_from_reader(&output_path, 0o644, &mut reader);

        // Assert
        assert_that!(actual).is_ok();
        assert_that!(output_path).is_file_with_content("file contents");
    }

    #[test]
    fn write_file_from_reader_should_set_permissions_correctly() {
        // Arrange
        let temp_dir = TempDir::new().unwrap();

        let output_path = temp_dir.path().join("executable");
        let mut reader = Cursor::new([]);

        let input = 0o755;

        // Act
        let actual = write_file_from_reader(&output_path, input, &mut reader);

        // Assert
        assert_that!(actual).is_ok();

        let permissions = fs::metadata(output_path).unwrap().permissions().mode() & 0o777;

        assert_that!(permissions).is_equal_to(input);
    }

    #[test]
    fn detect_archive_type_should_return_err_when_file_is_not_an_archive() {
        // Arrange
        let input = PathBuf::from("file.txt");

        // Act
        let actual = detect_archive_type(&input);

        // Assert
        assert_that!(&actual).is_err();
        assert_that!(matches!(
            actual.unwrap_err(),
            DetectArchiveTypeError { path: _ }
        ))
        .is_true();
    }

    #[test]
    fn detect_archive_type_should_return_err_when_file_contains_archive_extension_in_name() {
        // Arrange
        let input = PathBuf::from("zip.txt");

        // Act
        let actual = detect_archive_type(&input);

        // Assert
        assert_that!(&actual).is_err();
        assert_that!(matches!(
            actual.unwrap_err(),
            DetectArchiveTypeError { path: _ }
        ))
        .is_true();
    }

    #[test]
    fn detect_archive_type_should_detect_zip_extension() {
        // Arrange
        let input = PathBuf::from("archive.zip");

        // Act
        let actual = detect_archive_type(&input);

        // Assert
        assert_that!(actual)
            .to_value()
            .is_equal_to(ArchiveType::Zip);
    }

    #[test]
    fn detect_archive_type_should_detect_tar_extension() {
        // Arrange
        let input = PathBuf::from("archive.tar");

        // Act
        let actual = detect_archive_type(&input);

        // Assert
        assert_that!(actual)
            .to_value()
            .is_equal_to(ArchiveType::Tar);
    }

    #[test]
    fn detect_archive_type_should_detect_tar_gz_extension() {
        // Arrange
        let input = PathBuf::from("archive.tar.gz");

        // Act
        let actual = detect_archive_type(&input);

        // Assert
        assert_that!(actual)
            .to_value()
            .is_equal_to(ArchiveType::TarGz);
    }

    #[test]
    fn detect_archive_type_should_detect_tgz_extension() {
        // Arrange
        let input = PathBuf::from("archive.tgz");

        // Act
        let actual = detect_archive_type(&input);

        // Assert
        assert_that!(actual)
            .to_value()
            .is_equal_to(ArchiveType::TarGz);
    }

    #[test]
    fn detect_archive_type_should_detect_tar_zst_extension() {
        // Arrange
        let input = PathBuf::from("archive.tar.zst");

        // Act
        let actual = detect_archive_type(&input);

        // Assert
        assert_that!(actual)
            .to_value()
            .is_equal_to(ArchiveType::TarZst);
    }

    #[test]
    fn detect_archive_type_should_detect_tzst_extension() {
        // Arrange
        let input = PathBuf::from("archive.tzst");

        // Act
        let actual = detect_archive_type(&input);

        // Assert
        assert_that!(actual)
            .to_value()
            .is_equal_to(ArchiveType::TarZst);
    }

    #[test]
    fn detect_archive_type_should_detect_tar_xz_extension() {
        // Arrange
        let input = PathBuf::from("archive.tar.xz");

        // Act
        let actual = detect_archive_type(&input);

        // Assert
        assert_that!(actual)
            .to_value()
            .is_equal_to(ArchiveType::TarXz);
    }

    #[test]
    fn detect_archive_type_should_detect_txz_extension() {
        // Arrange
        let input = PathBuf::from("archive.txz");

        // Act
        let actual = detect_archive_type(&input);

        // Assert
        assert_that!(actual)
            .to_value()
            .is_equal_to(ArchiveType::TarXz);
    }

    #[test]
    fn detect_archive_type_should_detect_tbz2_extension() {
        // Arrange
        let input = PathBuf::from("archive.tbz2");

        // Act
        let actual = detect_archive_type(&input);

        // Assert
        assert_that!(actual)
            .to_value()
            .is_equal_to(ArchiveType::TarBz2);
    }

    #[test]
    fn detect_archive_type_should_detect_tar_bz2_extension() {
        // Arrange
        let input = PathBuf::from("archive.tar.bz2");

        // Act
        let actual = detect_archive_type(&input);

        // Assert
        assert_that!(actual)
            .to_value()
            .is_equal_to(ArchiveType::TarBz2);
    }

    #[test]
    fn detect_archive_type_should_detect_7z_extension() {
        // Arrange
        let input = PathBuf::from("archive.7z");

        // Act
        let actual = detect_archive_type(&input);

        // Assert
        assert_that!(actual)
            .to_value()
            .is_equal_to(ArchiveType::SevenZip);
    }

    #[test]
    fn detect_archive_type_should_detect_rar_extension() {
        // Arrange
        let input = PathBuf::from("archive.rar");

        // Act
        let actual = detect_archive_type(&input);

        // Assert
        assert_that!(actual)
            .to_value()
            .is_equal_to(ArchiveType::Rar);
    }

    #[test]
    fn is_archive_should_return_false_when_input_is_not_archive() {
        // Arrange
        let input = PathBuf::from("file.txt");

        // Act
        let actual = is_archive(&input);

        // Assert
        assert_that!(actual).is_false();
    }

    #[test]
    fn is_archive_should_return_true_when_input_is_archive() {
        // Arrange
        let input = PathBuf::from("archive.zip");

        // Act
        let actual = is_archive(&input);

        // Assert
        assert_that!(actual).is_true();
    }

    #[test]
    fn archive_stem_should_return_none_when_normal_file_given() {
        // Arrange
        let input = PathBuf::from("file.txt");

        // Act
        let actual = archive_stem(&input);

        // Assert
        assert_that!(actual).is_none();
    }

    #[test]
    fn archive_stem_should_return_file_name_when_archive_given() {
        // Arrange
        let input = PathBuf::from("file.tar.gz");

        // Act
        let actual = archive_stem(&input);

        // Assert
        assert_that!(actual).to_value().is_equal_to("file");
    }
}
