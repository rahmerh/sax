use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use bzip2::Compression as BzCompression;
use bzip2::write::BzEncoder;
use flate2::Compression as GzCompression;
use flate2::write::GzEncoder;
use sevenz_rust::{SevenZArchiveEntry, SevenZWriter};
use thiserror::Error;
use xz2::write::XzEncoder;
use zip::write::SimpleFileOptions;
use zstd::stream::write::Encoder as ZstdEncoder;

use crate::config::CreatePrefs;
use crate::progress::{NoProgress, ProgressReporter};
use crate::util::fs::{ArchiveType, DetectArchiveTypeError, detect_archive_type};

#[derive(Debug, Error)]
pub enum ArchiveError {
    #[error(transparent)]
    UnsupportedArchiveType(#[from] DetectArchiveTypeError),
    #[error("creating {format} archives is not supported")]
    UnsupportedCreation { format: &'static str },
    #[error("input path has no file name: {path}")]
    MissingInputName { path: PathBuf },
    #[error("unsupported input type: {path}")]
    UnsupportedInputType { path: PathBuf },
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("ZIP archive error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("7z archive error: {0}")]
    SevenZip(#[from] sevenz_rust::Error),
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn create_archive(
    inputs: Vec<&Path>,
    out: &Path,
    prefs: &CreatePrefs,
) -> Result<(), ArchiveError> {
    let mut progress = NoProgress;
    create_archive_with_progress(inputs, out, prefs, &mut progress)
}

pub fn create_archive_with_progress(
    inputs: Vec<&Path>,
    out: &Path,
    _prefs: &CreatePrefs,
    progress: &mut dyn ProgressReporter,
) -> Result<(), ArchiveError> {
    let archive_type = detect_archive_type(out)?;

    if archive_type == ArchiveType::Rar {
        return Err(ArchiveError::UnsupportedCreation { format: "RAR" });
    }

    let total = total_input_bytes(&inputs)?;

    if let Some(parent) = out.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }

    progress.start("Archiving", Some(total));

    let result = match archive_type {
        ArchiveType::Zip => create_zip(&inputs, out, progress),
        ArchiveType::Tar => create_tar_file(&inputs, out, progress),
        ArchiveType::TarGz => create_tar_gz(&inputs, out, progress),
        ArchiveType::TarXz => create_tar_xz(&inputs, out, progress),
        ArchiveType::TarBz2 => create_tar_bz2(&inputs, out, progress),
        ArchiveType::TarZst => create_tar_zst(&inputs, out, progress),
        ArchiveType::SevenZip => create_7z(&inputs, out, progress),
        ArchiveType::Rar => unreachable!("RAR creation was rejected before creating output"),
    };

    progress.finish();
    result
}

fn total_input_bytes(inputs: &[&Path]) -> Result<u64, ArchiveError> {
    let mut total = 0;

    for input in inputs {
        total += total_path_bytes(input)?;
    }

    Ok(total)
}

fn total_path_bytes(path: &Path) -> Result<u64, ArchiveError> {
    let metadata = fs::symlink_metadata(path)?;

    if metadata.is_dir() {
        let mut total = 0;

        for entry in fs::read_dir(path)? {
            total += total_path_bytes(&entry?.path())?;
        }

        Ok(total)
    } else if metadata.is_file() {
        Ok(metadata.len())
    } else {
        Err(ArchiveError::UnsupportedInputType {
            path: path.to_path_buf(),
        })
    }
}

fn create_zip(
    inputs: &[&Path],
    out: &Path,
    progress: &mut dyn ProgressReporter,
) -> Result<(), ArchiveError> {
    let mut archive = zip::ZipWriter::new(File::create(out)?);

    for input in inputs {
        let name = input_name(input)?;
        append_zip_path(&mut archive, input, &name, progress)?;
    }

    archive.finish()?;
    Ok(())
}

fn append_zip_path(
    archive: &mut zip::ZipWriter<File>,
    source: &Path,
    archive_path: &Path,
    progress: &mut dyn ProgressReporter,
) -> Result<(), ArchiveError> {
    let metadata = fs::symlink_metadata(source)?;
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(metadata.permissions().mode());

    progress.set_message(archive_path.display().to_string());

    if metadata.is_dir() {
        archive.add_directory_from_path(archive_path, options)?;

        for entry in fs::read_dir(source)? {
            let entry = entry?;
            append_zip_path(
                archive,
                &entry.path(),
                &archive_path.join(entry.file_name()),
                progress,
            )?;
        }
    } else if metadata.is_file() {
        archive.start_file_from_path(archive_path, options)?;
        copy_with_progress(File::open(source)?, archive, progress)?;
    } else {
        return Err(ArchiveError::UnsupportedInputType {
            path: source.to_path_buf(),
        });
    }

    Ok(())
}

fn create_tar_file(
    inputs: &[&Path],
    out: &Path,
    progress: &mut dyn ProgressReporter,
) -> Result<(), ArchiveError> {
    create_tar(inputs, File::create(out)?, progress)?;
    Ok(())
}

fn create_tar_gz(
    inputs: &[&Path],
    out: &Path,
    progress: &mut dyn ProgressReporter,
) -> Result<(), ArchiveError> {
    let encoder = GzEncoder::new(File::create(out)?, GzCompression::default());
    create_tar(inputs, encoder, progress)?.finish()?;
    Ok(())
}

fn create_tar_xz(
    inputs: &[&Path],
    out: &Path,
    progress: &mut dyn ProgressReporter,
) -> Result<(), ArchiveError> {
    let encoder = XzEncoder::new(File::create(out)?, 6);
    create_tar(inputs, encoder, progress)?.finish()?;
    Ok(())
}

fn create_tar_bz2(
    inputs: &[&Path],
    out: &Path,
    progress: &mut dyn ProgressReporter,
) -> Result<(), ArchiveError> {
    let encoder = BzEncoder::new(File::create(out)?, BzCompression::default());
    create_tar(inputs, encoder, progress)?.finish()?;
    Ok(())
}

fn create_tar_zst(
    inputs: &[&Path],
    out: &Path,
    progress: &mut dyn ProgressReporter,
) -> Result<(), ArchiveError> {
    let encoder = ZstdEncoder::new(File::create(out)?, 0)?;
    create_tar(inputs, encoder, progress)?.finish()?;
    Ok(())
}

fn create_tar<W: Write>(
    inputs: &[&Path],
    writer: W,
    progress: &mut dyn ProgressReporter,
) -> Result<W, ArchiveError> {
    let mut archive = tar::Builder::new(writer);
    archive.follow_symlinks(false);

    for input in inputs {
        let name = input_name(input)?;
        append_tar_path(&mut archive, input, &name, progress)?;
    }

    Ok(archive.into_inner()?)
}

fn append_tar_path<W: Write>(
    archive: &mut tar::Builder<W>,
    source: &Path,
    archive_path: &Path,
    progress: &mut dyn ProgressReporter,
) -> Result<(), ArchiveError> {
    let metadata = fs::symlink_metadata(source)?;
    progress.set_message(archive_path.display().to_string());

    if metadata.is_dir() {
        archive.append_dir(archive_path, source)?;

        for entry in fs::read_dir(source)? {
            let entry = entry?;
            append_tar_path(
                archive,
                &entry.path(),
                &archive_path.join(entry.file_name()),
                progress,
            )?;
        }
    } else if metadata.is_file() {
        let mut header = tar::Header::new_gnu();
        header.set_metadata(&metadata);
        header.set_path(archive_path)?;
        header.set_cksum();

        let mut reader = ProgressRead::new(File::open(source)?, progress);
        archive.append(&header, &mut reader)?;
    } else {
        return Err(ArchiveError::UnsupportedInputType {
            path: source.to_path_buf(),
        });
    }

    Ok(())
}

fn create_7z(
    inputs: &[&Path],
    out: &Path,
    progress: &mut dyn ProgressReporter,
) -> Result<(), ArchiveError> {
    let mut archive = SevenZWriter::create(out)?;

    for input in inputs {
        let name = input_name(input)?;
        append_7z_path(&mut archive, input, &name, progress)?;
    }

    archive.finish()?;
    Ok(())
}

fn append_7z_path(
    archive: &mut SevenZWriter<File>,
    source: &Path,
    archive_path: &Path,
    progress: &mut dyn ProgressReporter,
) -> Result<(), ArchiveError> {
    let metadata = fs::symlink_metadata(source)?;
    let name = archive_path.to_string_lossy().replace('\\', "/");
    let entry = SevenZArchiveEntry::from_path(source, name);

    progress.set_message(archive_path.display().to_string());

    if metadata.is_dir() {
        archive.push_archive_entry::<&[u8]>(entry, None)?;

        for child in fs::read_dir(source)? {
            let child = child?;
            append_7z_path(
                archive,
                &child.path(),
                &archive_path.join(child.file_name()),
                progress,
            )?;
        }
    } else if metadata.is_file() {
        let reader = ProgressRead::new(File::open(source)?, progress);
        archive.push_archive_entry(entry, Some(reader))?;
    } else {
        return Err(ArchiveError::UnsupportedInputType {
            path: source.to_path_buf(),
        });
    }

    Ok(())
}

fn copy_with_progress<R: Read, W: Write>(
    reader: R,
    writer: &mut W,
    progress: &mut dyn ProgressReporter,
) -> io::Result<u64> {
    let mut reader = ProgressRead::new(reader, progress);
    io::copy(&mut reader, writer)
}

struct ProgressRead<'a, R> {
    inner: R,
    progress: &'a mut dyn ProgressReporter,
}

impl<'a, R> ProgressRead<'a, R> {
    fn new(inner: R, progress: &'a mut dyn ProgressReporter) -> Self {
        Self { inner, progress }
    }
}

impl<R: Read> Read for ProgressRead<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let bytes = self.inner.read(buf)?;

        if bytes > 0 {
            self.progress.inc(bytes as u64);
        }

        Ok(bytes)
    }
}

fn input_name(path: &Path) -> Result<PathBuf, ArchiveError> {
    path.file_name()
        .map(PathBuf::from)
        .ok_or_else(|| ArchiveError::MissingInputName {
            path: path.to_path_buf(),
        })
}

#[cfg(test)]
mod tests {
    use kernal::prelude::*;
    use tempfile::TempDir;

    use crate::{
        config::{ExtractPrefs, OutputModes},
        extract::extract_archive,
    };

    use super::*;

    #[derive(Default)]
    struct RecordingProgress {
        started: Option<(&'static str, Option<u64>)>,
        bytes: u64,
        finished: bool,
    }

    impl ProgressReporter for RecordingProgress {
        fn start(&mut self, action: &'static str, total: Option<u64>) {
            self.started = Some((action, total));
        }

        fn set_message(&mut self, _message: String) {}

        fn inc(&mut self, bytes: u64) {
            self.bytes += bytes;
        }

        fn finish(&mut self) {
            self.finished = true;
        }
    }

    #[test]
    fn create_archive_should_return_err_when_trying_to_archive_as_rar() {
        // Arrange
        let temp_dir = TempDir::new().unwrap();
        let inputs = vec![];
        let archive = temp_dir.path().join("out.rar");
        let prefs = CreatePrefs {};

        // Act
        let actual = create_archive(inputs, &archive, &prefs);

        // Assert
        assert_that!(&actual).is_err();
        assert_that!(matches!(
            actual.unwrap_err(),
            ArchiveError::UnsupportedCreation { format: "RAR" }
        ))
        .is_true();
    }

    #[test]
    fn create_archive_with_progress_should_report_input_bytes() {
        // Arrange
        let temp_dir = TempDir::new().unwrap();

        let input_dir = temp_dir.path().join("input");
        let nested_dir = input_dir.join("nested");
        fs::create_dir_all(&nested_dir).unwrap();
        fs::write(input_dir.join("root.txt"), "foo").unwrap();
        fs::write(nested_dir.join("inner.txt"), "hello").unwrap();

        let archive = temp_dir.path().join("out.zip");
        let prefs = CreatePrefs {};
        let mut progress = RecordingProgress::default();

        // Act
        create_archive_with_progress(vec![input_dir.as_path()], &archive, &prefs, &mut progress)
            .unwrap();

        // Assert
        assert_that!(progress.started).is_equal_to(Some(("Archiving", Some(8))));
        assert_that!(progress.bytes).is_equal_to(8);
        assert_that!(progress.finished).is_true();
    }

    #[test]
    fn create_archive_should_create_zip() {
        // Arrange
        let temp_dir = TempDir::new().unwrap();

        let file = temp_dir.path().join("file.txt");
        fs::write(&file, "foo").unwrap();

        let archive = temp_dir.path().join("out.zip");

        let prefs = CreatePrefs {};

        // Act
        create_archive(vec![file.as_path()], &archive, &prefs).unwrap();

        // Assert
        assert_that!(&archive).exists();

        let extracted_dir = temp_dir.path().join("extracted");
        extract_archive(
            &archive,
            &extracted_dir,
            &ExtractPrefs {
                strip_top_level_dir: false,
                output_mode: OutputModes::Manual,
            },
        )
        .unwrap();

        assert_that!(extracted_dir.join("file.txt")).is_file_with_content("foo");
    }

    #[test]
    fn create_archive_should_create_tar() {
        // Arrange
        let temp_dir = TempDir::new().unwrap();

        let file = temp_dir.path().join("file.txt");
        fs::write(&file, "foo").unwrap();

        let archive = temp_dir.path().join("out.tar");

        let prefs = CreatePrefs {};

        // Act
        create_archive(vec![file.as_path()], &archive, &prefs).unwrap();

        // Assert
        assert_that!(&archive).exists();

        let extracted_dir = temp_dir.path().join("extracted");
        extract_archive(
            &archive,
            &extracted_dir,
            &ExtractPrefs {
                strip_top_level_dir: false,
                output_mode: OutputModes::Manual,
            },
        )
        .unwrap();

        assert_that!(extracted_dir.join("file.txt")).is_file_with_content("foo");
    }

    #[test]
    fn create_archive_should_create_tar_gz() {
        // Arrange
        let temp_dir = TempDir::new().unwrap();

        let file = temp_dir.path().join("file.txt");
        fs::write(&file, "foo").unwrap();

        let archive = temp_dir.path().join("out.tar.gz");

        let prefs = CreatePrefs {};

        // Act
        create_archive(vec![file.as_path()], &archive, &prefs).unwrap();

        // Assert
        assert_that!(&archive).exists();

        let extracted = temp_dir.path().join("extracted");
        extract_archive(
            &archive,
            &extracted,
            &ExtractPrefs {
                strip_top_level_dir: false,
                output_mode: OutputModes::Manual,
            },
        )
        .unwrap();

        assert_that!(extracted.join("file.txt")).is_file_with_content("foo");
    }

    #[test]
    fn create_archive_should_create_tar_xz() {
        // Arrange
        let temp_dir = TempDir::new().unwrap();

        let file = temp_dir.path().join("file.txt");
        fs::write(&file, "foo").unwrap();

        let archive = temp_dir.path().join("out.tar.xz");

        let prefs = CreatePrefs {};

        // Act
        create_archive(vec![file.as_path()], &archive, &prefs).unwrap();

        // Assert
        assert_that!(&archive).exists();

        let extracted_dir = temp_dir.path().join("extracted");
        extract_archive(
            &archive,
            &extracted_dir,
            &ExtractPrefs {
                strip_top_level_dir: false,
                output_mode: OutputModes::Manual,
            },
        )
        .unwrap();

        assert_that!(extracted_dir.join("file.txt")).is_file_with_content("foo");
    }

    #[test]
    fn create_archive_should_create_tar_bz2() {
        // Arrange
        let temp_dir = TempDir::new().unwrap();

        let file = temp_dir.path().join("file.txt");
        fs::write(&file, "foo").unwrap();

        let archive = temp_dir.path().join("out.tar.bz2");

        let prefs = CreatePrefs {};

        // Act
        create_archive(vec![file.as_path()], &archive, &prefs).unwrap();

        // Assert
        assert_that!(&archive).exists();

        let extracted_dir = temp_dir.path().join("extracted");
        extract_archive(
            &archive,
            &extracted_dir,
            &ExtractPrefs {
                strip_top_level_dir: false,
                output_mode: OutputModes::Manual,
            },
        )
        .unwrap();

        assert_that!(extracted_dir.join("file.txt")).is_file_with_content("foo");
    }

    #[test]
    fn create_archive_should_create_tar_zst() {
        // Arrange
        let temp_dir = TempDir::new().unwrap();

        let file = temp_dir.path().join("file.txt");
        fs::write(&file, "foo").unwrap();

        let archive = temp_dir.path().join("out.tar.zst");

        let prefs = CreatePrefs {};

        // Act
        create_archive(vec![file.as_path()], &archive, &prefs).unwrap();

        // Assert
        assert_that!(&archive).exists();

        let extracted = temp_dir.path().join("extracted");
        extract_archive(
            &archive,
            &extracted,
            &ExtractPrefs {
                strip_top_level_dir: false,
                output_mode: OutputModes::Manual,
            },
        )
        .unwrap();

        assert_that!(extracted.join("file.txt")).is_file_with_content("foo");
    }

    #[test]
    fn create_archive_should_create_7z() {
        // Arrange
        let temp_dir = TempDir::new().unwrap();

        let file = temp_dir.path().join("file.txt");
        fs::write(&file, "foo").unwrap();

        let archive = temp_dir.path().join("out.7z");

        let prefs = CreatePrefs {};

        // Act
        create_archive(vec![file.as_path()], &archive, &prefs).unwrap();

        // Assert
        assert_that!(&archive).exists();

        let extracted = temp_dir.path().join("extracted");
        extract_archive(
            &archive,
            &extracted,
            &ExtractPrefs {
                strip_top_level_dir: false,
                output_mode: OutputModes::Manual,
            },
        )
        .unwrap();

        assert_that!(extracted.join("file.txt")).is_file_with_content("foo");
    }

    #[test]
    fn create_archive_should_create_missing_directories_when_required() {
        // Arrange
        let temp_dir = TempDir::new().unwrap();

        let file = temp_dir.path().join("file.txt");
        fs::write(&file, "foo").unwrap();

        let archive = temp_dir.path().join("missing-dir").join("out.zip");

        let prefs = CreatePrefs {};

        // Act
        create_archive(vec![file.as_path()], &archive, &prefs).unwrap();

        // Assert
        assert_that!(&archive).exists();
    }
}
