# Archives

Which formats sax knows, and how it extracts and creates each one.
Code: `src/extract.rs`, `src/archive.rs`, `src/util/fs.rs`

## Formats

| Format | Suffixes | Extract | Create | Compression on create |
|---|---|---|---|---|
| zip | `.zip` | yes | yes | deflate |
| tar | `.tar` | yes | yes | none |
| tar + gzip | `.tar.gz`, `.tgz` | yes | yes | gzip default (6) |
| tar + xz | `.tar.xz`, `.txz` | yes | yes | xz level 6 |
| tar + bzip2 | `.tar.bz2`, `.tbz2` | yes | yes | bzip2 default (6) |
| tar + zstd | `.tar.zst`, `.tzst` | yes | yes | zstd level 0 (library default, 3) |
| 7z | `.7z` | yes | yes | sevenz-rust default |
| RAR | `.rar` | yes | no: `creating RAR archives is not supported` | - |

## Detection
- By file name only, never by content: lowercased name must end in a suffix above with something before
  it (`.zip` alone isn't an archive). List: `ARCHIVE_SUFFIXES` in `src/util/fs.rs`.
- Multi-part RAR: pass the first part (`x.part01.rar`); `unrar` finds the rest beside it.

## Extracting
- zip and tar entries go through `safe_join`: empty, absolute or `..` paths fail the whole extract with
  `unsafe ZIP|TAR path '<p>': <reason>`. `.` parts are dropped.
- 7z and RAR are handed to their library whole (`decompress_file`, `extract_all`); sax doesn't check
  their paths itself.
- zip and tar extract only directories and regular files. Symlinks, hard links and devices are skipped
  silently.
- Permissions: the entry's mode masked to `0o777` (no setuid/sticky), `0o644` if it has none. Zip
  directories keep default permissions.
- Existing files in the output are overwritten.

## Top-level stripping
With `strip_top_level_dir: true` (default):
1. Extract into `<out>/.extract-<nanos>-staging`.
2. If the staging dir holds exactly one entry and it's a directory, its contents are the result;
   otherwise the staging dir's contents are.
3. Move the result into `<out>` with `rename`, then remove the staging dir.
- Moving onto an existing non-empty directory of the same name fails.
- If extraction fails, the staging dir is left behind in `<out>`.

## Creating
- tar formats: `tar::Builder` with `follow_symlinks(false)`, so symlinks are stored as links.
- zip and 7z walk the tree themselves and store files and dirs with their Unix mode; any other entry
  (a symlink, a socket) fails with `unsupported input type: <path>`.
- An input with no file name (`/`, `..`) fails with `input path has no file name: <path>`.

## Tests
- Fixtures are embedded as bytes from `samples/` (one per format, plus `flatten-sample.tar` for stripping
  and a two-part RAR).
