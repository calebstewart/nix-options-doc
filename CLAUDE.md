# CLAUDE.md

Navigation guide for this repo. The `README.md` covers installation, the full CLI flag
reference, and usage examples — this file covers *how to work in the code*.

References below point to files and named symbols, never line numbers — line numbers drift
stale on every merge, while a symbol like `parse_attrset` or `anchor_slug` is one `rg` command
away regardless of how the file has changed. Don't re-add line numbers.

## What this is

A Rust CLI that generates NixOS module option documentation (Markdown / HTML / JSON / CSV).

The one design fact that explains everything else: **it parses Nix source statically with
[`rnix`](https://github.com/nix-community/rnix-parser); it never evaluates it.** No `nix` binary
is involved. That's why the code is full of best-effort fallbacks to raw source text, why
unparseable input degrades to zero options instead of an error, and why values guarded by
`mkIf`/`mkMerge` are reported in their statically-visible form rather than resolved.

## Commands

**There is no `cargo`/`rustc` on this machine's PATH** — the toolchain comes from the flake
dev shell. Prefix every command with `nix develop --command`, or enter the shell once:

```bash
nix develop                                    # interactive; or…
nix develop --command cargo test               # …one-shot, for scripted/agent use
```

(Note `nix` is a shell function here that wraps `nix develop`/`nix run`; in a non-interactive
Bash tool call use `command nix develop --command …` so the wrapper doesn't swallow it.)

The rest of this section lists the commands themselves, minus that prefix:

```bash
cargo build                                    # cargo build --release for the LTO profile
cargo nextest run                              # what CI runs; the dev shell provides it
cargo fmt -- --check
cargo clippy --all-targets                     # CI runs this with RUSTFLAGS=-Dwarnings

cargo run -- --path ./some/modules --format html --out /tmp/out.html
RUST_LOG=debug cargo run -- --path ./some/modules   # parser skip reasons land here
```

`nix build` builds the package with the *same* toolchain as the dev shell — `flake.nix`
shares one `rustToolchain` between both on purpose so they can't drift.

MSRV is **1.88** (`Cargo.toml`); the `msrv` CI job pins `1.88.0` on Ubuntu and runs
`cargo check --locked` — that `--locked` is the only place `Cargo.lock` staleness is
caught. `cargo nextest run` covers macOS/Ubuntu/Windows on stable; `fmt`/`clippy` run once
on Ubuntu/stable. CI also runs `cargo deny check` — see
`.github/workflows/run-tests.yml` and `deny.toml`. Note `deny.toml` bans `git2`/`openssl`/
`libssh2-sys`, which is why the `gix` dependency uses the rustls HTTP transport.

Pure Cargo project — no justfile, Makefile, or package.json.

## Pipeline

Four stages, all defined in `src/lib.rs`, called in order from `src/main.rs`:

| Stage | Where | What it does |
|---|---|---|
| `prepare_path` | `src/lib.rs` | Returns the local path, or shallow-clones a git URL via `gix` into a `TempDir`. `main` holds that `TempDir` alive as `_temp_dir` so the clone isn't deleted mid-run. |
| `collect_options` | `src/lib.rs` | `WalkDir` + `filter_entry`/`utils::should_traverse_entry` (prunes hidden dirs below the root) → `utils::should_process_file` → **`nix_files.sort()`** → rayon `par_iter` → `utils::process_nix_file`. Then merges same-named options into one `OptionDoc` with multiple `Declaration`s. |
| `filter_options` | `src/lib.rs` | prefix / type / `--search` regex / `--has-*` filters, then `--strip-prefix`, then `renamed_to` anchor resolution, then `--out-prefix` on every declaration path. |
| `generate_doc` | `src/lib.rs` | Optional sort, then dispatch on `OutputFormat`. |

Shell completions short-circuit before any of this, in `src/main.rs`.

Per-file drill-down inside `collect_options`:

```
utils::process_nix_file          src/utils.rs
  ├─ nix_call::collect_aliases / collect_let_bindings
  ├─ parser::visit_node          src/parser.rs   walks the tree, builds the dotted prefix,
  │    └─ parser::parse_attrset  src/parser.rs   folds mkIf conditions into scope
  │                                              ^ the big dispatch on node kind — most
  │                                              parser work goes here
  └─ parser::find_deprecations   src/parser.rs   mkRenamedOptionModule / mkRemovedOptionModule
```

Parallelism is rayon over *files only*; everything downstream is single-threaded.

## Modules

| File | Responsibility |
|---|---|
| `src/main.rs` | Thin driver: init logging, parse CLI, run the four stages, write to stdout or `--out`. |
| `src/lib.rs` | Crate root. CLI structs, `OptionDoc`/`Declaration`, and the four pipeline functions. |
| `src/parser.rs` | rnix tree traversal. Recognizes `mkOption`, `mkEnableOption`, `mkPackageOption`, `mkMerge`, `mkIf`, `let…in`, `with`, and `<expr> // { … }` overrides. Handles inline submodule recursion (bounded by `MAX_SUBMODULE_DEPTH` in `src/types.rs`, to guard against cyclic submodule types) and `freeformType`. |
| `src/types.rs` | Formats Nix type expressions into nixpkgs-style prose (`nullOr` → "null or …", `listOf` → "list of …"). Falls back to raw dedented source rather than guessing. |
| `src/nix_call.rs` | Low-level AST helpers: unwind curried `NODE_APPLY` chains into `(fn_name, args)`, attrset key lookup, `let`-binding and alias collection. |
| `src/utils.rs` | Per-file driver, description cleanup (admonitions, dedent, `literalExpression` unwrapping), `${var}` replacement, walkdir filtering, anchor slugs, `KEY=VALUE` arg parser. |
| `src/error.rs` | `NixDocError` (thiserror) and its `From` conversions. |
| `src/generate/markdown.rs` | `## [\`name\`](file#Lnn)` headings with an explicit `<a id>` anchor above each. Renders **Condition:** and **Also declared in:** sections. |
| `src/generate/json.rs` | 13 lines — `serde_json::to_string_pretty` over `&[OptionDoc]`. |
| `src/generate/csv.rs` | Fixed header; missing values become `-`; descriptions flattened to one line. |
| `src/generate/html/` | Single self-contained HTML file. See below. |

### HTML generator

- `src/generate/html/mod.rs` orchestrates: configures comrak, builds the search index and
  category index, then splices their JSON into the script template at the `__SEARCH_INDEX__` /
  `__CATEGORY_INDEX__` placeholders. This is a single-pass `split_once`
  over the *pristine* template rather than sequential `String::replace` calls, so inserted
  data is never rescanned and a description that happens to contain literal placeholder text
  can't get treated as a second substitution target. The JSON itself has every `<` escaped
  to `\u003c` (via `push_script_safe_json`) before insertion, so it can contain no `<!--`,
  `<script`, or `</script` sequence — the escape is what actually keeps the `<script>`
  element from being prematurely closed or driven into script-data-escaped state, not the
  placeholder mechanism.
- `src/generate/html/render.rs` is per-option markup: `CATEGORIES` (canonical legend order),
  the `classify_type` heuristic, `render_option`.
- `src/generate/html/template.rs` is ~670 lines of **static** CSS/JS scaffolding — the CSS
  custom-property design system (light, dark, explicit `data-theme`, reduced-motion), the
  no-flash theme restore script, and the client-side regex search, which runs in a Web Worker
  built from a Blob URL (`workerSource`) with an inline fallback. Editing HTML output usually
  means editing this file.

## Key types

- **`OptionDoc`** (`src/lib.rs`) — `name`, `description`, `nix_type`, `default_value`,
  `example`, `renamed_to`, `declarations: Vec<Declaration>`.
- **`Declaration`** (`src/lib.rs`) — `file_path`, `line_number`, plus `description`
  (populated only when this declaration's differs from the primary one) and `condition`
  (the *source text* of the guarding `mkIf`, joined with `&&` when nested).

  ⚠️ The serde derives on these two **are** the JSON output schema. Adding or renaming a
  field changes public output.

- **CLI** — `Cli` (`src/lib.rs`) flattens four `Args` structs: `IoOptions`,
  `GitOptions`, `FilterOptions`, `UtilityOptions`. `OutputFormat` is also in `src/lib.rs`.
- **`NixDocError`** (`src/error.rs`).

## Non-obvious conventions

- **Tests are `include!`d, not a normal test target.** `src/tests/tests.rs` (~2,730 lines,
  55 tests) is textually included into a `#[cfg(test)] mod tests` in `src/lib.rs` so it can
  reach private items via `use super::*`.
- **No fixture files.** Every test builds a `tempfile::TempDir`, writes inline Nix with
  `create_test_file` (`src/tests/tests.rs`), calls `collect_options`, and asserts on the
  resulting `Vec<OptionDoc>`. Follow that pattern for new tests.
- **Two error types.** `src/parser.rs` returns `Box<dyn Error + Send + Sync>`; `src/lib.rs`
  and `src/generate/` use `NixDocError`. The bridge is the `From<Box<dyn Error + Send + Sync>>`
  impl in `src/error.rs`.
- **Graceful degradation is deliberate.** Unreadable or unparseable files log an error and
  yield zero options (`utils::process_nix_file`); an invalid `--search` regex logs and skips
  filtering. Don't "fix" these into hard failures.
- **`nix_files.sort()` (`src/lib.rs`) is load-bearing.** It exists for determinism — sort
  order decides which declaration becomes the primary one after merging.
- **`utils::anchor_slug` (`src/utils.rs`) is shared by Markdown and HTML on purpose**, so links
  resolve identically in both regardless of any renderer's own heading-slug algorithm. Change
  both or neither.
- **Regexes** are compiled once into `static … LazyLock<Regex>` (`VAR_REGEX`,
  `ADMONITION_REGEX`, `PREFIX_REGEX`, `DIRECTIVE_REGEX` in `src/utils.rs`).
- **No cargo features exist** — no `[features]` table, no `#[cfg(feature = …)]` anywhere. That's why CI dropped `cargo-hack`; if a `[features]` table is ever added, reinstate `cargo hack check --each-feature` in `run-tests.yml` (there's a comment there saying so).
- **Doc style.** Public functions carry rustdoc with `# Arguments` / `# Returns`, and
  non-obvious decisions get long inline comments explaining *why*. Match that.

## Recipes

- **An option isn't being detected** → run with `RUST_LOG=debug`. `parser::parse_attrset`
  logs both unhandled node kinds and unrecognized option functions.
- **Support a new option builtin** (`mkFooOption`) → add a match arm in `parse_attrset`
  (`src/parser.rs`), near the `mkOption` arm. Add a test.
- **Support a new type combinator** → `types::format_ident` (`src/types.rs`) for bare
  identifiers, `types::format_call` (`src/types.rs`) for applied ones.
- **Add an output format** → new file under `src/generate/`, re-export from
  `src/generate/mod.rs`, add an `OutputFormat` variant (`src/lib.rs`) and a matching
  `generate_doc` arm (`src/lib.rs`).
- **Add a CLI flag** → the relevant flattened `Args` struct in `src/lib.rs`, then wire it into
  `filter_options` or `generate_doc`. The README's flag table needs a matching row.
- **Change HTML appearance** → `src/generate/html/template.rs` (CSS/JS) or
  `src/generate/html/render.rs` (per-option markup).

## Fork notes

Hard fork of `Thunderbottom/nix-options-doc`. Only `origin` is configured (this fork); there is
no upstream remote and no upstream-compatibility constraint on changes.

`Cargo.toml` still has `publish = false` and the original author/repository metadata, and
`.github/workflows/build-release.yml` still points at the upstream release flow — worth
revisiting before cutting any release from this fork.
