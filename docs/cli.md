# CLI

How `sax <PATH>...` decides between extracting and creating, where output goes, and the config file.
Code: `src/cli.rs`, `src/config.rs`, `src/util/prompt.rs`, `src/theme.rs`, `src/main.rs`

## Picking the action
- No paths: prints help (name, version, usage, supported formats) and exits 0.
- First path has an archive name (see `docs/archives.md` → "Detection") and exists: **extract** it.
- Otherwise, first path exists: **create**.
- Otherwise: error `Cannot infer desired functionality from arguments.`
- Edge case: `sax a.zip b.zip` with `a.zip` existing extracts `a.zip` into a directory named `b.zip`.
  An existing archive can only be archived when it isn't the first path.

## Extracting
The output directory depends on `extract_prefs.output_mode`:

| Mode | 1 path | 2 paths | 3+ paths |
|---|---|---|---|
| `auto` | `<archive dir>/<archive stem>` | error, too many | error, too many |
| `prompt` (default) | asks `Where do you want to extract to?` | 2nd path | error, too many |
| `manual` | error, output directory is required | 2nd path | error, too many |

- Stem drops the full suffix, keeping case: `Foo.tar.gz` → `Foo`.
- Output path exists but isn't a directory: error `<path> is not a folder`.
- A missing output directory is created, with parents.

## Creating
- Last path doesn't exist and has an archive name: it is the archive, the rest are inputs.
- Otherwise every path is an input and sax asks `Where do you want to create the archive?`
  (in every output mode).
- Then, in order: every input must exist (`<path> does not exist`), the archive must not exist
  (`archive already exists: <path>`), and it must have an archive name
  (`<path> is not a supported archive name`).
- Duplicate inputs are added once. Missing parent dirs of the archive are created.
- Each input goes in at the archive root under its own file name; directories keep their tree.

## Prompts
- `dialoguer` text input with the theme in `src/theme.rs` (cyan `➜`/`›`, red `✘`).
- Behind the `Prompt` trait so tests mock it (`MockPrompt`).

## Config
- File: `$XDG_CONFIG_HOME/sax/config.yaml` if that var is set and non-empty, else `~/.config/sax/config.yaml`.
- Missing: written with defaults on the first run that has paths. Invalid YAML: error
  `invalid config YAML in <path>`. Missing keys take their default.

```yaml
create_prefs: {}            # no options yet
extract_prefs:
  strip_top_level_dir: true # see docs/archives.md → "Top-level stripping"
  output_mode: prompt       # auto | prompt | manual
```

## Errors and exit codes
- Any error prints one line to stderr and exits 1.
- Format failures are wrapped: `could not extract archive <a> to <out>: <cause>` and
  `could not create archive <out>: <cause>`. Causes are in `ExtractError` (`src/extract.rs`) and
  `ArchiveError` (`src/archive.rs`).
