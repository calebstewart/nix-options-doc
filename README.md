<h1 align="center">nix-options-doc</h1>

A command-line tool that generates comprehensive, multi-format documentation for NixOS module options.

A live example of the generated documentation can be found at
[Thunderbottom/flakes](https://github.com/Thunderbottom/flakes/blob/main/options.md),
produced by the upstream project this tool was forked from.

## Why?

NixOS configurations can be complex, with numerous modules and options that need clear documentation. While many Nix projects showcase elegant module documentation, I couldn't find a dedicated tool to generate it without extra setup. The usual approach, like `nixOptionsDoc`, evaluates your module tree with `nix eval`/`nix-instantiate` and feeds the result through a separate renderer, which requires a working Nix installation and `nix eval`, and a module tree that actually evaluates cleanly as a whole.

`nix-options-doc` rather parses the Nix source directly instead of evaluating it, using [rnix](https://github.com/nix-community/rnix-parser), which means:

- **No Nix required.** It's a single static binary. No `nix` install, `nix eval`, no `documentation.nix` boilerplate configuration. Point it at a directory or a git repository, and get docs.
- **Works on anything, evaluable or not.** Since nothing gets evaluated, it can document work-in-progress modules, a module tree that isn't wired into a full flake or NixOS config, or someone else's repo you have no intention of building.
- **Works on any remote repository** straight from a Git URL (HTTPS or SSH, with branch/tag selection), no manual cloning required.
- **Simple and intuitive.** One command, sensible defaults, and four output formats (Markdown, HTML, JSON, CSV) out of the box.

But this also means: since nothing is evaluated, values that depend on runtime conditions (`mkIf`, cross-file `mkMerge`, and similar) are shown in their statically-visible form rather than fully resolved (aka "evaluated"). For the common case of documenting your personal project's option type, default, description, and declaration site, that rarely matters in practice.

## Features

- **Multiple Output Formats**: Generate documentation in Markdown, HTML, JSON, or CSV
- **Rich Documentation**: Captures option names, types, default values, examples, descriptions, and source references
- **Improved Type Detection**: Intelligent parsing of complex Nix types with human-friendly output
- **Repository Support**: Works with both local paths and remote Git repositories (with branch/tag selection)
- **Variable Interpolation**: Handles `${namespace}` style variables with configurable replacements
- **Admonition Support**: Renders warning, note, and important blocks in both Markdown and HTML output
- **Instant Search**: The HTML output includes a built-in, client-side regex search bar
- **Filtering Capabilities**: Filter by prefix, type, search term, or other criteria
- **Robust Error Handling**: Detailed error messages and graceful recovery from parsing issues
- **Parallel Processing**: Fast performance with multi-threaded file processing
- **Progress Visibility**: Optional progress bar for monitoring documentation generation
- **Shell Completions**: Generate completion scripts for bash, zsh, fish, elvish, and powershell

## Installation

### Pre-built Binary

Pre-built binaries are attached to every [release](https://github.com/calebstewart/nix-options-doc/releases):
Linux x86_64 and aarch64 (statically linked against musl), macOS on Intel and Apple
Silicon, and Windows x86_64. Each archive ships with a matching `.sha256` file.

### Using Cargo

```bash
$ cargo install --git https://github.com/calebstewart/nix-options-doc
```

Or build from source:

```bash
$ git clone https://github.com/calebstewart/nix-options-doc.git
$ cd nix-options-doc
$ cargo build --release
```

### Using Nix

```bash
$ nix build github:calebstewart/nix-options-doc
$ ./result/bin/nix-options-doc --path /etc/nixos --out nixos-options.md
```

## Usage

### Basic Usage

```bash
# Generate documentation for current directory, output to stdout
$ nix-options-doc

# Generate documentation for a specific path
$ nix-options-doc --path ./nixos/modules --out modules-doc.md

# Generate sorted documentation
$ nix-options-doc --path ./nixos/modules --sort

# Generate HTML documentation
$ nix-options-doc --format html --out modules.html

# Show progress bar during generation
$ nix-options-doc --progress
```

### Advanced Usage

```bash
# Filter options by prefix
$ nix-options-doc --filter-by-prefix services.nginx

# Exclude specific directories
$ nix-options-doc --exclude-dir templates,tests

# Replace variables in Nix modules
$ nix-options-doc --replace namespace=snowflake --replace system=x86_64-linux

# Only include options with descriptions
$ nix-options-doc --has-description

# Strip common prefix from option names
$ nix-options-doc --strip-prefix options.services

# A bare prefix means the same thing: `options.` is added for you
$ nix-options-doc --strip-prefix services
```

### Working with Git Repositories

```bash
# Clone and document a GitHub repository (HTTPS)
$ nix-options-doc --path https://github.com/user/repo.git

# Use specific branch or tag
$ nix-options-doc --path git@github.com:user/repo.git --branch feature-branch

# Shallow clone with custom depth
$ nix-options-doc --path git://example.com/repo.git --depth 5
```

Declaration links (e.g. `modules/nginx.nix#L42`) are always relative to
wherever the source was read from, whether that's a local path or a
freshly cloned repository. If you host the generated docs somewhere other
than alongside that source tree - a docs site, GitHub Pages, anywhere not
serving the repo itself - those links won't resolve on their own. Use
`--out-prefix` to rewrite them to point at the real source instead:

```bash
# Point declaration links at the repo's GitHub blob view
$ nix-options-doc --path https://github.com/user/repo.git \
    --out-prefix https://github.com/user/repo/blob/main
```

### Command Line Options

```
Usage: nix-options-doc [OPTIONS]

Options:
      --generate-completions <SHELL>  Generate a shell completion script and exit [possible values: bash, elvish, fish, powershell, zsh]
  -p, --path <PATH>                Local path or remote git repository URL [default: .]
  -o, --out <OUT>                  Path to output file or 'stdout' [default: stdout]
  -f, --format <FORMAT>            Output format [default: markdown] [possible values: markdown, json, html, csv]
  -s, --sort                       Sort options alphabetically
      --out-prefix <PATH>          Prefix declaration links with this URL or path
  -b, --branch <BRANCH>            Git branch or tag to use (for remote repositories)
  -d, --depth <DEPTH>              Git commit depth for shallow clones [default: 1]
      --filter-by-prefix <PREFIX>  Filter options by prefix (e.g. "services.nginx")
      --filter-by-type <NIX_TYPE>  Filter options by type (e.g. "bool", "string")
      --search <OPTION>            Search in option names and descriptions
      --has-default                Only show options that have a default value
      --has-description            Only show options that have a description
      --replace <KEY=VALUE>        Replace variables in Nix modules (can be used multiple times)
      --strip-prefix [<PREFIX>]    Remove a prefix from option names; a bare prefix means options.<PREFIX> [without a value: options.]
  -e, --exclude-dir <EXCLUDE_DIR>  Directories to exclude from processing
      --follow-symlinks            Enable traversing through symbolic links (can reach hidden directories; see the note below)
      --progress                   Show progress bar
  -h, --help                       Print help
  -V, --version                    Print version
```

**Note:** hidden files and directories (names starting with `.`, such as
`.git`, `.direnv`, `.cache`) are skipped during traversal. The path you pass
to `--path` is exempt, so `--path ./.config/nixos` still works.

This pruning is by name, and it applies to the tree being walked rather than
to the targets that symbolic links resolve to. With `--follow-symlinks`, a
link whose own name is not hidden is followed even when it points into a
hidden directory, and the options behind it are documented — that applies
both to a link to a directory and to a link to a single `.nix` file. This is
deliberate: passing the flag is consent to leave the visible tree, and layouts
that symlink out to a hidden source directory (dotfiles/stow-style trees) rely
on it. The practical consequence is that scanning an untrusted tree with
`--follow-symlinks` can read files the visible tree does not appear to expose.
Use `--exclude-dir` to skip a link you do not want followed.

### Shell Completions

```bash
# Bash (add to ~/.bashrc, or write to a file sourced from there)
$ nix-options-doc --generate-completions bash > /etc/bash_completion.d/nix-options-doc

# Zsh (needs to be somewhere on $fpath)
$ nix-options-doc --generate-completions zsh > ~/.zsh/completions/_nix-options-doc

# Fish
$ nix-options-doc --generate-completions fish > ~/.config/fish/completions/nix-options-doc.fish
```

`elvish` and `powershell` are also supported.

### Logging and Exit Status

`nix-options-doc` logs at the `warn` level by default, so warnings — a path that
yielded no options, filters that matched nothing, a directory that could not be
traversed — are printed to stderr without any extra setup. Set `RUST_LOG` to
override (`RUST_LOG=debug` for parser detail, `RUST_LOG=error` to silence
warnings).

The output document is always produced, even when zero options are found: `--out`
is written (and any file already there is overwritten) and stdout gets the empty
document. The exit status is `0` in that case — an option-less tree is not an
error. Only real failures exit non-zero: a `--path` that cannot be read at all, a failed
clone, an unwritable output file. A root path that cannot be traversed is one of those
failures — the run stops, nothing is written, and an existing `--out` file is left intact
rather than being replaced by an empty document. A directory *below* the root that cannot
be read stays a warning: the rest of the tree is still documented and the run exits `0`.

## Output Examples

### Markdown Format

The Markdown output uses a heading-based structure for each option:

```markdown
## [`services.nginx.enable`](<modules/nginx/default.nix#L25>)

Whether to enable the Nginx web server.

**Type:** `boolean`

**Default:** `false`

**Example:** `true`
```

### Admonition Support

The tool properly renders admonition blocks in Nix module descriptions:

```nix
# In your Nix file:
description = ''
  Regular description text.

  ::: {.warning}
  This setting can impact system security.
  :::
'';
```

Will be rendered in Markdown as:

```markdown
Regular description text.

> [!WARNING]
> This setting can impact system security.
```

And in HTML with proper styling.

## Development

### Prerequisites

- Rust 1.88 or later
- Git (for repository cloning features)

### Building and Testing

```bash
# Build the project
$ cargo build

# Run tests
$ cargo test

# Run with debug logging
$ RUST_LOG=debug cargo run -- --path /path/to/nixos/modules
```

### Project Structure

- `src/generate/` - Output format generators (Markdown, HTML, JSON, CSV)
- `src/parser.rs` - Nix file parser using rnix syntax tree
- `src/types.rs` - Nix type expression formatting
- `src/nix_call.rs` - Function-call resolution and local alias detection
- `src/utils.rs` - Helper functions for file processing and text manipulation
- `src/error.rs` - Error type definitions and handling
- `src/lib.rs` - Core functions and CLI structure
- `src/main.rs` - Command-line interface

## Contributing

Contributions are welcome! Feel free to submit a Pull Request or open an issue.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/AmazingFeature`)
3. Commit your changes (`git commit -m 'Add some AmazingFeature'`)
4. Push to the branch (`git push origin feature/AmazingFeature`)
5. Open a Pull Request

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
