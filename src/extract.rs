use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use bzip2::read::BzDecoder;
use flate2::read::GzDecoder;
use tar::EntryType;
use thiserror::Error;
use xz2::read::XzDecoder;
use zstd::stream::read::Decoder as ZstdDecoder;

use crate::fs_utils::{
    UnsafePathError, move_dir_contents, safe_join, set_permissions, single_child_dir_or_self,
    write_file_from_reader,
};

#[derive(Debug, Error)]
pub enum ExtractError {
    #[error("'{path}' does not exist")]
    ArchiveDoesNotExist { path: PathBuf },
    #[error("unsupported archive type: {path}")]
    UnsupportedArchiveType { path: PathBuf },
    #[error("failed to create output directory '{path}': {source}")]
    CreateOutputDir {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to read output directory '{path}': {source}")]
    ReadOutputDir {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("could not handle files in staging directory '{path}': {source}")]
    MoveStagedFiles {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("unsafe {format} path '{path}': {source}")]
    UnsafeArchivePath {
        format: &'static str,
        path: PathBuf,
        #[source]
        source: UnsafePathError,
    },
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("ZIP archive error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("7z archive error: {0}")]
    SevenZip(#[from] sevenz_rust::Error),
    #[error("RAR archive error: {0}")]
    Rar(#[from] unrar_ng::error::UnrarError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArchiveType {
    Zip,
    Tar,
    TarGz,
    TarXz,
    TarBz2,
    TarZst,
    SevenZip,
    Rar,
}

pub fn extract_archive(
    path: &Path,
    out: &Path,
    strip_top_level_dir: bool,
) -> Result<(), ExtractError> {
    if !path.exists() {
        return Err(ExtractError::ArchiveDoesNotExist {
            path: path.to_path_buf(),
        });
    }

    fs::create_dir_all(out).map_err(|source| ExtractError::CreateOutputDir {
        path: out.to_path_buf(),
        source,
    })?;

    if strip_top_level_dir {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let staging_dir = out.join(format!(".extract-{ts}-staging"));
        fs::create_dir_all(&staging_dir).map_err(|source| ExtractError::CreateOutputDir {
            path: out.to_path_buf(),
            source,
        })?;

        extract_archive_to(path, &staging_dir)?;

        let source_dir = single_child_dir_or_self(&staging_dir).map_err(|source| {
            ExtractError::ReadOutputDir {
                path: staging_dir.to_path_buf(),
                source,
            }
        })?;

        move_dir_contents(&source_dir, out).map_err(|source| ExtractError::MoveStagedFiles {
            path: source_dir,
            source,
        })?;

        fs::remove_dir_all(&staging_dir)?;

        Ok(())
    } else {
        extract_archive_to(path, out)
    }
}

fn extract_archive_to(path: &Path, out: &Path) -> Result<(), ExtractError> {
    match detect_archive_type(path)? {
        ArchiveType::Zip => extract_zip(path, out),
        ArchiveType::Tar => extract_tar(File::open(path)?, out),
        ArchiveType::TarGz => extract_tar(GzDecoder::new(File::open(path)?), out),
        ArchiveType::TarXz => extract_tar(XzDecoder::new(File::open(path)?), out),
        ArchiveType::TarBz2 => extract_tar(BzDecoder::new(File::open(path)?), out),
        ArchiveType::TarZst => extract_tar(ZstdDecoder::new(File::open(path)?)?, out),
        ArchiveType::SevenZip => extract_7z(path, out),
        ArchiveType::Rar => extract_rar(path, out),
    }
}

fn extract_zip(path: &Path, out: &Path) -> Result<(), ExtractError> {
    let file = File::open(path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let out_path = safe_join(out, Path::new(entry.name())).map_err(|source| {
            ExtractError::UnsafeArchivePath {
                format: "ZIP",
                path: PathBuf::from(entry.name()),
                source,
            }
        })?;

        if entry.is_dir() {
            fs::create_dir_all(&out_path)?;
            continue;
        }

        if !entry.is_file() {
            continue;
        }

        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mode = entry.unix_mode().unwrap_or(0o644);
        write_file_from_reader(&out_path, mode, &mut entry)?;
    }

    Ok(())
}

fn extract_tar<R>(reader: R, out: &Path) -> Result<(), ExtractError>
where
    R: Read,
{
    let mut archive = tar::Archive::new(reader);

    for entry in archive.entries()? {
        let mut entry = entry?;
        let entry_path = entry.path()?.into_owned();
        let out_path =
            safe_join(out, &entry_path).map_err(|source| ExtractError::UnsafeArchivePath {
                format: "TAR",
                path: entry_path.clone(),
                source,
            })?;
        let entry_type = entry.header().entry_type();
        let mode = entry.header().mode().unwrap_or(0o644);

        match entry_type {
            EntryType::Directory => {
                fs::create_dir_all(&out_path)?;
                set_permissions(&out_path, mode)?;
            }
            EntryType::Regular => {
                if let Some(parent) = out_path.parent() {
                    fs::create_dir_all(parent)?;
                }

                write_file_from_reader(&out_path, mode, &mut entry)?;
            }
            _ => {}
        }
    }

    Ok(())
}

fn extract_7z(path: &Path, out: &Path) -> Result<(), ExtractError> {
    sevenz_rust::decompress_file(path, out)?;
    Ok(())
}

fn extract_rar(path: &Path, out: &Path) -> Result<(), ExtractError> {
    unrar_ng::Archive::new(path)
        .open_for_processing()?
        .extract_all(out)?;
    Ok(())
}

fn detect_archive_type(path: &Path) -> Result<ArchiveType, ExtractError> {
    let name = path
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or_default()
        .to_lowercase();

    match name.as_str() {
        name if name.ends_with(".zip") => Ok(ArchiveType::Zip),
        name if name.ends_with(".tar") => Ok(ArchiveType::Tar),
        name if name.ends_with(".tar.gz") || name.ends_with(".tgz") => Ok(ArchiveType::TarGz),
        name if name.ends_with(".tar.xz") || name.ends_with(".txz") => Ok(ArchiveType::TarXz),
        name if name.ends_with(".tar.bz2") || name.ends_with(".tbz2") => Ok(ArchiveType::TarBz2),
        name if name.ends_with(".tar.zst") || name.ends_with(".tzst") => Ok(ArchiveType::TarZst),
        name if name.ends_with(".7z") => Ok(ArchiveType::SevenZip),
        name if name.ends_with(".rar") => Ok(ArchiveType::Rar),
        _ => Err(ExtractError::UnsupportedArchiveType {
            path: path.to_path_buf(),
        }),
    }
}

pub fn is_archive(path: &Path) -> bool {
    path.exists() && detect_archive_type(path).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use sha1::{Digest, Sha1};
    use std::io::Write;
    use tempfile::TempDir;
    use zip::write::SimpleFileOptions;

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

            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn is_archive_should_return_true_when_correct_archive_name_given() {
        // Arrange
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("a.tar.gz");
        File::create(&path).unwrap();

        // Act
        let actual = is_archive(&path);

        // Assert
        assert!(actual);
    }

    #[test]
    fn is_archive_should_return_false_when_incorrect_archive_name_given() {
        // Arrange
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("foo");
        File::create(&path).unwrap();

        // Act
        let actual = is_archive(&path);

        // Assert
        assert!(!actual);
    }

    #[test]
    fn extract_archive_should_return_extract_error_when_archive_doesnt_exist() {
        // Arrange
        let input = PathBuf::from("/tmp/doesnt-exist.zip");
        let out = TempDir::new().unwrap();

        // Act
        let result = extract_archive(&input, out.path(), true);

        // Assert
        assert!(result.is_err());
        match result.err().unwrap() {
            ExtractError::ArchiveDoesNotExist { path: actual_path } => {
                assert_eq!(actual_path, input);
            }
            other => panic!("expected ArchiveDoesNotExist, got {other:?}"),
        }
    }

    #[test]
    fn extract_archive_should_extract_zip_into_output_directory() {
        // Arrange
        let dir = TempDir::new().unwrap();
        let archive_path = dir.path().join("a.zip");
        let out_dir = dir.path().join("out");

        create_zip(
            &archive_path,
            &[
                ("root.txt", "hello"),
                ("nested/", ""),
                ("nested/inner.txt", "world"),
            ],
        );

        // Act
        extract_archive(&archive_path, &out_dir, true).unwrap();

        // Assert
        assert_eq!(
            fs::read_to_string(out_dir.join("root.txt")).unwrap(),
            "hello"
        );

        assert!(out_dir.join("nested").is_dir());
        assert_eq!(
            fs::read_to_string(out_dir.join("nested/inner.txt")).unwrap(),
            "world"
        );
    }

    #[test]
    fn extract_archive_should_extract_tar_into_output_directory() {
        // Arrange
        let dir = TempDir::new().unwrap();
        let archive_path = dir.path().join("a.tar");
        let out_dir = dir.path().join("out");

        create_tar(
            &archive_path,
            &[("root.txt", "hello"), ("nested/inner.txt", "world")],
        );

        // Act
        extract_archive(&archive_path, &out_dir, true).unwrap();

        // Assert
        assert_eq!(
            fs::read_to_string(out_dir.join("root.txt")).unwrap(),
            "hello"
        );

        assert!(out_dir.join("nested").is_dir());
        assert_eq!(
            fs::read_to_string(out_dir.join("nested/inner.txt")).unwrap(),
            "world"
        );
    }

    #[test]
    fn extract_archive_should_extract_tar_gz_into_output_directory() {
        // Arrange
        let dir = TempDir::new().unwrap();
        let tar_path = dir.path().join("a.tar");
        let archive_path = dir.path().join("a.tar.gz");
        let out_dir = dir.path().join("out");

        create_tar(
            &tar_path,
            &[("root.txt", "hello"), ("nested/inner.txt", "world")],
        );
        create_gzip(&tar_path, &archive_path);

        // Act
        extract_archive(&archive_path, &out_dir, true).unwrap();

        // Assert
        assert_eq!(
            fs::read_to_string(out_dir.join("root.txt")).unwrap(),
            "hello"
        );

        assert!(out_dir.join("nested").is_dir());
        assert_eq!(
            fs::read_to_string(out_dir.join("nested/inner.txt")).unwrap(),
            "world"
        );
    }

    #[test]
    fn extract_archive_should_strip_single_top_level_directory() {
        // Arrange
        let dir = TempDir::new().unwrap();
        let archive_path = dir.path().join("a.tar");
        let out_dir = dir.path().join("out");

        create_tar(
            &archive_path,
            &[
                ("package/root.txt", "hello"),
                ("package/nested/inner.txt", "world"),
            ],
        );

        // Act
        extract_archive(&archive_path, &out_dir, true).unwrap();

        // Assert
        assert_eq!(
            fs::read_to_string(out_dir.join("root.txt")).unwrap(),
            "hello"
        );
        assert_eq!(
            fs::read_to_string(out_dir.join("nested/inner.txt")).unwrap(),
            "world"
        );
        assert!(!out_dir.join("package").exists());
    }

    #[test]
    fn extract_archive_should_preserve_single_top_level_directory_when_strip_is_false() {
        // Arrange
        let dir = TempDir::new().unwrap();
        let archive_path = dir.path().join("a.tar");
        let out_dir = dir.path().join("out");

        create_tar(
            &archive_path,
            &[
                ("package/root.txt", "hello"),
                ("package/nested/inner.txt", "world"),
            ],
        );

        // Act
        extract_archive(&archive_path, &out_dir, false).unwrap();

        // Assert
        assert_eq!(
            fs::read_to_string(out_dir.join("package/root.txt")).unwrap(),
            "hello"
        );
        assert_eq!(
            fs::read_to_string(out_dir.join("package/nested/inner.txt")).unwrap(),
            "world"
        );
    }

    #[test]
    fn extract_archive_should_extract_7z_into_output_directory() {
        // Arrange
        let dir = TempDir::new().unwrap();
        let input_dir = dir.path().join("input");
        let nested_dir = input_dir.join("nested");
        let archive_path = dir.path().join("a.7z");
        let out_dir = dir.path().join("out");

        fs::create_dir_all(&nested_dir).unwrap();
        fs::write(input_dir.join("root.txt"), "hello").unwrap();
        fs::write(nested_dir.join("inner.txt"), "world").unwrap();

        sevenz_rust::compress_to_path(&input_dir, &archive_path).unwrap();

        // Act
        extract_archive(&archive_path, &out_dir, true).unwrap();

        // Assert
        assert_eq!(
            fs::read_to_string(out_dir.join("root.txt")).unwrap(),
            "hello"
        );

        assert!(out_dir.join("nested").is_dir());
        assert_eq!(
            fs::read_to_string(out_dir.join("nested/inner.txt")).unwrap(),
            "world"
        );
    }

    #[test]
    fn extract_archive_should_extract_rar_into_output_directory() {
        // Arrange
        let dir = TempDir::new().unwrap();
        let archive_path = dir.path().join("test.part01.rar");
        let out_dir = dir.path().join("out");

        create_fixture_archive(&archive_path, TEST_RAR_PART_1);
        create_fixture_archive(&dir.path().join("test.part02.rar"), TEST_RAR_PART_2);

        // Act
        extract_archive(&archive_path, &out_dir, true).unwrap();

        // Assert
        let actual = fs::read(out_dir.join("test.txt")).unwrap();
        let digest = Sha1::digest(&actual);
        assert_eq!(
            format!("{digest:x}"),
            "4da7f88f69b44a3fdb705667019a65f4c6e058a3"
        );
    }

    fn create_zip(path: &Path, entries: &[(&str, &str)]) {
        let file = File::create(path).unwrap();
        let mut archive = zip::ZipWriter::new(file);
        let file_options = SimpleFileOptions::default().unix_permissions(0o644);
        let dir_options = SimpleFileOptions::default().unix_permissions(0o755);

        for (name, content) in entries {
            if name.ends_with('/') {
                archive.add_directory(*name, dir_options).unwrap();
                continue;
            }

            archive.start_file(*name, file_options).unwrap();
            archive.write_all(content.as_bytes()).unwrap();
        }

        archive.finish().unwrap();
    }

    fn create_tar(path: &Path, entries: &[(&str, &str)]) {
        let file = File::create(path).unwrap();
        let mut archive = tar::Builder::new(file);

        for (name, content) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_path(name).unwrap();
            header.set_mode(0o644);
            header.set_size(content.len() as u64);
            header.set_cksum();
            archive
                .append(&header, content.as_bytes())
                .expect("append tar entry");
        }

        archive.finish().unwrap();
    }

    fn create_gzip(input_path: &Path, output_path: &Path) {
        let input = fs::read(input_path).unwrap();
        let output = File::create(output_path).unwrap();
        let mut encoder = GzEncoder::new(output, Compression::default());

        encoder.write_all(&input).unwrap();
        encoder.finish().unwrap();
    }

    fn create_fixture_archive(path: &Path, encoded: &str) {
        fs::write(path, decode_base64(encoded)).unwrap();
    }

    fn decode_base64(input: &str) -> Vec<u8> {
        let mut out = Vec::new();
        let mut buffer = 0u32;
        let mut bits = 0u8;

        for byte in input.bytes() {
            let value = match byte {
                b'A'..=b'Z' => byte - b'A',
                b'a'..=b'z' => byte - b'a' + 26,
                b'0'..=b'9' => byte - b'0' + 52,
                b'+' => 62,
                b'/' => 63,
                b'=' => break,
                b'\n' | b'\r' | b'\t' | b' ' => continue,
                other => panic!("invalid base64 byte {other}"),
            } as u32;

            buffer = (buffer << 6) | value;
            bits += 6;

            if bits >= 8 {
                bits -= 8;
                out.push((buffer >> bits) as u8);
                buffer &= (1 << bits) - 1;
            }
        }

        out
    }

    const TEST_RAR_PART_1: &str = "UmFyIRoHAQBt4SgnCwEFBwEGAQGAgIAAEP5UsygCEwvhhgAEv8UApIMC4JlDmoADAQh0ZXN0LnR4dAoDEyO9GGi6v4cPz7QlBEVUMzUFU/JQNHL7mGuFIr5z/J6B4bcvXfL11LjidU6p04HF6D3xMapGjQBCKOxFtYcNiyPkggDd6gOAjCMD/5QACQANA/frHrT1r6z629b+uPXPrr1jyJ3Fx3Gx3Hx3Ix3Jx3Kxx+bmdzdJp5RtpNJpNJpNJpNZrNfPaW1ms1ms1mszMzMz5yLZmZmZmZtNptNpt588ttNptNpvN5vN5vN/PrbbzebzicTicTicTjzpluJxOZzOZzOZzOZz52u3M6nU6nU6nU6nU6nXgL7BnvsG++w899hHvsKe+wn32HvvsK++wt782/0bXeDjwc+D94OvB/8Hfg87Hvc7zb6TSaTSaTSaTSaTWazWazWazWazWazMzMzMzMzMzMzM2m02m02m36/jzH6fj57+I+n9jaf8f2/p5j+//juvx83/+x/K7g/r/v7r/pp/T/jDv+/PW/fbg+Ly9rzeMC+MDeMD+MEeME+MHvGCvGD/jBf1gz6wb4w8+sI+sKfWE/WHv1hX6wt9YX+sMeMM97r7QxMTExMTExMTE0mk0mk0mk0mk0mk1ms1ms1ms1ms1mszMzMzMzMzMzMzNptNptNptNptNptN5vN5vN5vN5vN5vOJxOJxOJxOJxOJxOZzOZzOZzOZzOZzOp1Op1Op1Op1Op5fOJiYmJiYmJiYn+vwF13j7QxMTExMTExMTy+cTExMTExMTExMTExMTExMTExMTExMTExMTExMTExMTExMTE7yBBiIECBAgQIECBAgQIECBAgQIFdp8Wutda611rrXWutdYECBAgRpBGk0mk0mk0hpECBAgQIECBAgQIECBAgQIECBAgQIECBAgQIECBAgV2vxa611rrXWutda611gRiCBAg1iBAgQI1msFbzLPx9oazWazWeXygQIECBAgQIECBAgQIECBAgQIECBAgQIECBXZ+LXWutda611rrXWusCBAgQIECBAgQIECBAjMzMwRmZmZmGYgQIECBAgQIECBAgQIECBAgQIECBArtvi11rrXWutda611rrAgRiCBBtECBAgQIECBAgQIECBG02m02m0NvL52m07y+0BAgQIECBAgQIECBAgQIECBArt/i11rrXWutda611rrAgQIECBAgQIECBAgQIECBAgQIECBAgQIItHUSYDBQQBAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==";

    const TEST_RAR_PART_2: &str = "UmFyIRoHAQCpwsTKDAEFBwMBBgEBgICAAKB9hWAoAgsLyIEABL/FAKSDApFhDOCAAwEIdGVzdC50eHQKAxMjvRhour+HD0G8QbxAgQIECBAgQIECBAgV3Hxa611rrXWutda611gQIEYgg4iBAgQIECBAgQIECBAgQIECBAgQIECBAgQI45+5faHM8vlAgQIOIgQIECu5+LXWutda611rrXWusCBAgQIECBAgQIECBAgQIECBAgQIECBAgQIECBAgQIECBAg5nfgfmBAgQdRXWutda611rrXWusCBAgRiHUQIECBAgQIECBAgQIECBAgQIECBAgQIECBAgQIECDqdTqdTqdTqdTqdTrr5dvOWHXdWUQMFBAA=";
}
