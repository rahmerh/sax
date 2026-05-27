use std::env;
use std::error::Error;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{self, Read, Write};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};
use std::process;

use bzip2::read::BzDecoder;
use flate2::read::GzDecoder;
use tar::EntryType;
use xz2::read::XzDecoder;
use zstd::stream::read::Decoder as ZstdDecoder;

type SaxResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

const VERSION: &str = "0.1.0";

const USAGE: &str = r#"sax - Smart archiving and extracting utility

Usage:
  sax <archive> <out>
  sax --help
  sax --version

Arguments:
  <archive>  Archive file to extract.
  <out>      Directory to extract into. Created if it does not exist.

Supported archive formats:
  zip, tar, tar.gz, tgz, tar.xz, txz, tar.bz2, tbz2, tar.zst, tzst, 7z, rar

Examples:
  sax photos.zip photos/
  sax backup.tar.gz restored/
"#;

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

fn main() {
    if let Err(err) = run(env::args().skip(1), &mut io::stdout()) {
        eprintln!("Something went wrong: {err}");
        process::exit(1);
    }
}

fn run<I, S>(args: I, stdout: &mut dyn Write) -> SaxResult<()>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let args: Vec<String> = args.into_iter().map(Into::into).collect();

    if args.is_empty() {
        stdout.write_all(USAGE.as_bytes())?;
        return Ok(());
    }

    if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") {
        stdout.write_all(USAGE.as_bytes())?;
        return Ok(());
    }

    if args.len() == 1 && args[0] == "--version" {
        writeln!(stdout, "sax {VERSION}")?;
        return Ok(());
    }

    if args.len() == 2 {
        let archive = Path::new(&args[0]);
        let out = Path::new(&args[1]);

        if !is_archive(archive) {
            return Err(format!("{} is not an archive", archive.display()).into());
        }

        extract_archive(archive, out).map_err(|err| {
            format!(
                "could not extract archive {} to {}: {err}",
                archive.display(),
                out.display()
            )
            .into()
        })
    } else if args.len() > 1 {
        Err("creating archives is not implemented".into())
    } else {
        Err("could not determine what you want to do".into())
    }
}

fn extract_archive(path: &Path, out: &Path) -> SaxResult<()> {
    if !path.exists() {
        return Err(format!("'{}' does not exist", path.display()).into());
    }

    fs::create_dir_all(out)
        .map_err(|err| format!("failed to create '{}': {err}", out.display()))?;

    match detect_archive_type(path)? {
        ArchiveType::Zip => extract_zip(path, out),
        ArchiveType::Tar => extract_tar(File::open(path)?, out),
        ArchiveType::TarGz => extract_tar(GzDecoder::new(File::open(path)?), out),
        ArchiveType::TarXz => extract_tar(XzDecoder::new(File::open(path)?), out),
        ArchiveType::TarBz2 => extract_tar(BzDecoder::new(File::open(path)?), out),
        ArchiveType::TarZst => extract_tar(ZstdDecoder::new(File::open(path)?)?, out),
        other => Err(format!("unsupported archive type: {other:?}").into()),
    }
}

fn extract_zip(path: &Path, out: &Path) -> SaxResult<()> {
    let file = File::open(path).map_err(|err| format!("failed to read ZIP archive: {err}"))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|err| format!("failed to read ZIP archive: {err}"))?;

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let out_path = safe_join(out, Path::new(entry.name()))
            .map_err(|err| format!("unsafe ZIP path '{}': {err}", entry.name()))?;

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

fn extract_tar<R>(reader: R, out: &Path) -> SaxResult<()>
where
    R: Read,
{
    let mut archive = tar::Archive::new(reader);

    for entry in archive.entries()? {
        let mut entry = entry?;
        let entry_path = entry.path()?.into_owned();
        let out_path = safe_join(out, &entry_path)
            .map_err(|err| format!("unsafe TAR path '{}': {err}", entry_path.display()))?;
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

fn write_file_from_reader(path: &Path, mode: u32, reader: &mut dyn Read) -> SaxResult<()> {
    let mut file = File::create(path)?;
    io::copy(reader, &mut file)?;
    set_permissions(path, mode)?;
    Ok(())
}

#[cfg(unix)]
fn set_permissions(path: &Path, mode: u32) -> io::Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(mode & 0o777))
}

#[cfg(not(unix))]
fn set_permissions(_path: &Path, _mode: u32) -> io::Result<()> {
    Ok(())
}

fn detect_archive_type(path: &Path) -> SaxResult<ArchiveType> {
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
        _ => Err(format!("unsupported archive type: {}", path.display()).into()),
    }
}

fn is_archive(path: &Path) -> bool {
    path.exists() && detect_archive_type(path).is_ok()
}

fn safe_join(base: &Path, rel: &Path) -> SaxResult<PathBuf> {
    if rel.as_os_str().is_empty() {
        return Err("empty paths are not allowed".into());
    }
    if rel.is_absolute() {
        return Err("absolute paths are not allowed".into());
    }

    let mut clean = PathBuf::new();
    for component in rel.components() {
        match component {
            Component::Normal(part) => clean.push(part),
            Component::CurDir => {}
            Component::ParentDir => return Err("path traversal ('..') is not allowed".into()),
            Component::RootDir | Component::Prefix(_) => {
                return Err("absolute paths are not allowed".into());
            }
        }
    }

    if clean.as_os_str().is_empty() {
        return Err("empty paths are not allowed".into());
    }

    Ok(base.join(clean))
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use std::io::Write;
    use tempfile::TempDir;
    use zip::write::SimpleFileOptions;

    #[test]
    fn run_prints_usage_without_arguments() {
        let mut stdout = Vec::new();

        run(Vec::<String>::new(), &mut stdout).unwrap();

        assert_eq!(String::from_utf8(stdout).unwrap(), USAGE);
    }

    #[test]
    fn run_prints_usage_with_help() {
        for flag in ["--help", "-h"] {
            let mut stdout = Vec::new();

            run([flag], &mut stdout).unwrap();

            assert_eq!(String::from_utf8(stdout).unwrap(), USAGE);
        }
    }

    #[test]
    fn run_prints_version() {
        let mut stdout = Vec::new();

        run(["--version"], &mut stdout).unwrap();

        assert_eq!(String::from_utf8(stdout).unwrap(), "sax 0.1.0\n");
    }

    #[test]
    fn detect_archive_types() {
        let tests = [
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

        for (name, want) in tests {
            assert_eq!(detect_archive_type(Path::new(name)).unwrap(), want);
        }
    }

    #[test]
    fn is_archive_requires_existing_supported_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("a.tar.gz");

        assert!(!is_archive(&path));

        fs::write(&path, b"archive").unwrap();

        assert!(is_archive(&path));
    }

    #[test]
    fn safe_join_rejects_unsafe_paths() {
        for rel in ["../escape", "/absolute", "nested/../../escape"] {
            assert!(safe_join(Path::new("/tmp/out"), Path::new(rel)).is_err());
        }
    }

    #[test]
    fn extract_archive_extracts_zip_files_into_output_directory() {
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

        extract_archive(&archive_path, &out_dir).unwrap();

        assert_file(&out_dir.join("root.txt"), "hello");
        assert_file(&out_dir.join("nested").join("inner.txt"), "world");
        assert!(out_dir.join("nested").is_dir());
    }

    #[test]
    fn extract_archive_extracts_tar_files_into_output_directory() {
        let dir = TempDir::new().unwrap();
        let archive_path = dir.path().join("a.tar");
        let out_dir = dir.path().join("out");

        create_tar(
            &archive_path,
            &[("root.txt", "hello"), ("nested/inner.txt", "world")],
        );

        extract_archive(&archive_path, &out_dir).unwrap();

        assert_file(&out_dir.join("root.txt"), "hello");
        assert_file(&out_dir.join("nested").join("inner.txt"), "world");
    }

    #[test]
    fn extract_archive_extracts_tar_gz_files_into_output_directory() {
        let dir = TempDir::new().unwrap();
        let tar_path = dir.path().join("a.tar");
        let archive_path = dir.path().join("a.tar.gz");
        let out_dir = dir.path().join("out");

        create_tar(
            &tar_path,
            &[("root.txt", "hello"), ("nested/inner.txt", "world")],
        );
        create_gzip(&tar_path, &archive_path);

        extract_archive(&archive_path, &out_dir).unwrap();

        assert_file(&out_dir.join("root.txt"), "hello");
        assert_file(&out_dir.join("nested").join("inner.txt"), "world");
    }

    #[test]
    fn extract_archive_fails_when_archive_does_not_exist() {
        let dir = TempDir::new().unwrap();
        let err = extract_archive(&dir.path().join("missing.zip"), &dir.path().join("out"))
            .unwrap_err()
            .to_string();

        assert!(err.contains("does not exist"));
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

    fn assert_file(path: &Path, want: &str) {
        assert_eq!(fs::read_to_string(path).unwrap(), want);
    }
}
