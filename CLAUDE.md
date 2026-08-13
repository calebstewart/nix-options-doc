# CLAUDE.md

Navigation guide for this repo. The `README.md` covers installation, the full CLI flag
reference, and usage examples — this file covers *how to work in the code*.

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
cargo test                                     # CI uses: cargo nextest run --all-features
cargo fmt -- --check
cargo clippy                                   # CI runs with RUSTFLAGS=-Dwarnings

cargo run -- --path ./some/modules --format html --out /tmp/out.html
RUST_LOG=debug cargo run -- --path ./some/modules   # parser skip reasons land here
```

`nix build` builds the package with the *same* toolchain as the dev shell — `flake.nix`
shares one `rustToolchain` between both on purpose so they can't drift.

MSRV is **1.88** (`Cargo.toml:6`); CI pins `1.88.0` across macOS/Ubuntu/Windows. CI also runs
`cargo hack check --each-feature --locked` and `cargo deny check` — see
`.github/workflows/run-tests.yml` and `deny.toml`. Note `deny.toml` bans `git2`/`openssl`/
`libssh2-sys`, which is why the `gix` dependency uses the rustls HTTP transport.

Pure Cargo project — no justfile, Makefile, or package.json.

## Pipeline

Four stages, all defined in `src/lib.rs`, called in order from `src/main.rs:15`:

| Stage | Where | What it does |
|---|---|---|
| `prepare_path` | `lib.rs:341` | Returns the local path, or shallow-clones a git URL via `gix` into a `TempDir`. `main` holds that `TempDir` alive as `_temp_dir` so the clone isn't deleted mid-run. |
| `collect_options` | `lib.rs:406` | `WalkDir` → `utils::should_process_file` → **`nix_files.sort()`** → rayon `par_iter` → `utils::process_nix_file`. Then merges same-named options into one `OptionDoc` with multiple `Declaration`s. |
| `filter_options` | `lib.rs:222` | prefix / type / `--search` regex / `--has-*` filters, then `--strip-prefix`, then `renamed_to` anchor resolution, then `--out-prefix` on every declaration path. |
| `generate_doc` | `lib.rs:568` | Optional sort, then dispatch on `OutputFormat`. |

Shell completions short-circuit before any of this, at `main.rs:19`.

Per-file drill-down inside `collect_options`:

```
utils::process_nix_file            utils.rs:279
  ├─ nix_call::collect_aliases / collect_let_bindings
  ├─ parser::visit_node            parser.rs:37    walks the tree, builds the dotted prefix,
  │    └─ parser::parse_attrset    parser.rs:295   folds mkIf conditions into scope
  │                                                ^ the big dispatch on node kind — most
  │                                                  parser work goes here
  └─ parser::find_deprecations     parser.rs:724   mkRenamedOptionModule / mkRemovedOptionModule
```

Parallelism is rayon over *files only*; everything downstream is single-threaded.

## Modules

| File | Responsibility |
|---|---|
| `src/main.rs` | Thin driver: init logging, parse CLI, run the four stages, write to stdout or `--out`. |
| `src/lib.rs` | Crate root. CLI structs, `OptionDoc`/`Declaration`, and the four pipeline functions. |
| `src/parser.rs` | rnix tree traversal. Recognizes `mkOption` (`:412`), `mkEnableOption`, `mkPackageOption`, `mkMerge`, `mkIf`, `let…in`, `with`, and `<expr> // { … }` overrides. Handles inline submodule recursion (bounded by `MAX_SUBMODULE_DEPTH`, `types.rs:21`, to guard against cyclic submodule types) and `freeformType`. |
| `src/types.rs` | Formats Nix type expressions into nixpkgs-style prose (`nullOr` → "null or …", `listOf` → "list of …"). Falls back to raw dedented source rather than guessing. |
| `src/nix_call.rs` | Low-level AST helpers: unwind curried `NODE_APPLY` chains into `(fn_name, args)`, attrset key lookup, `let`-binding and alias collection. |
| `src/utils.rs` | Per-file driver, description cleanup (admonitions, dedent, `literalExpression` unwrapping), `${var}` replacement, walkdir filtering, anchor slugs, `KEY=VALUE` arg parser. |
| `src/error.rs` | `NixDocError` (thiserror) and its `From` conversions. |
| `src/generate/markdown.rs` | `## [\`name\`](file#Lnn)` headings with an explicit `<a id>` anchor above each. Renders **Condition:** and **Also declared in:** sections. |
| `src/generate/json.rs` | 13 lines — `serde_json::to_string_pretty` over `&[OptionDoc]`. |
| `src/generate/csv.rs` | Fixed header; missing values become `-`; descriptions flattened to one line. |
| `src/generate/html/` | Single self-contained HTML file. See below. |

### HTML generator

- `html/mod.rs` orchestrates: configures comrak, builds the search index and category index,
  then string-substitutes `__SEARCH_INDEX__` / `__CATEGORY_INDEX__` into the script template
  (`mod.rs:97-107`). The `</` → `<\/` guard there stops a description from prematurely
  closing the `<script>` tag.
- `html/render.rs` is per-option markup: `CATEGORIES` (`:11`, canonical legend order),
  the `classify_type` heuristic (`:29`), `render_option`.
- `html/template.rs` is 663 lines of **static** CSS/JS scaffolding — the CSS custom-property
  design system (light, dark, explicit `data-theme`, reduced-motion), the no-flash theme
  restore script, and the client-side regex search, which runs in a Web Worker built from a
  Blob URL (`:65`) with an inline fallback. Editing HTML output usually means editing this file.

## Key types

- **`OptionDoc`** (`lib.rs:185`) — `name`, `description`, `nix_type`, `default_value`,
  `example`, `renamed_to`, `declarations: Vec<Declaration>`.
- **`Declaration`** (`lib.rs:160`) — `file_path`, `line_number`, plus `description`
  (populated only when this declaration's differs from the primary one) and `condition`
  (the *source text* of the guarding `mkIf`, joined with `&&` when nested).

  ⚠️ The serde derives on these two **are** the JSON output schema. Adding or renaming a
  field changes public output.

- **CLI** — `Cli` (`lib.rs:38`) flattens four `Args` structs: `IoOptions` (`:61`),
  `GitOptions` (`:88`), `FilterOptions` (`:103`), `UtilityOptions` (`:144`).
  `OutputFormat` is at `:26`.
- **`NixDocError`** (`error.rs:10`).

## Non-obvious conventions

- **Tests are `include!`d, not a normal test target.** `src/tests/tests.rs` (~1,600 lines,
  ~34 tests) is textually included into a `#[cfg(test)] mod tests` at `lib.rs:20-23` so it can
  reach private items via `use super::*`.
- **No fixture files.** Every test builds a `tempfile::TempDir`, writes inline Nix with
  `create_test_file` (`tests.rs:17`), calls `collect_options`, and asserts on the resulting
  `Vec<OptionDoc>`. Follow that pattern for new tests.
- **Two error types.** `parser.rs` returns `Box<dyn Error + Send + Sync>`; `lib.rs` and
  `generate/` use `NixDocError`. The bridge is the `From` impl at `error.rs:72`.
- **Graceful degradation is deliberate.** Unreadable or unparseable files log an error and
  yield zero options (`utils.rs:314`, `:329`); an invalid `--search` regex logs and skips
  filtering. Don't "fix" these into hard failures.
- **`nix_files.sort()` (`lib.rs:472`) is load-bearing.** It exists for determinism — sort
  order decides which declaration becomes the primary one after merging.
- **`utils::anchor_slug` (`utils.rs:28`) is shared by Markdown and HTML on purpose**, so links
  resolve identically in both regardless of any renderer's own heading-slug algorithm. Change
  both or neither.
- **Regexes** are compiled once into `static … LazyLock<Regex>` (`utils.rs:18`, `:65`,
  `:109`, `:155`).
- **No cargo features exist** — no `[features]` table, no `#[cfg(feature = …)]` anywhere.
- **Doc style.** Public functions carry rustdoc with `# Arguments` / `# Returns`, and
  non-obvious decisions get long inline comments explaining *why*. Match that.

## Recipes

- **An option isn't being detected** → run with `RUST_LOG=debug`. `parser.rs:702` logs
  unhandled node kinds and `parser.rs:614` logs unrecognized option functions.
- **Support a new option builtin** (`mkFooOption`) → add a match arm in `parse_attrset`
  (`parser.rs:295`), near the `mkOption` arm at `:412`. Add a test.
- **Support a new type combinator** → `types::format_ident` (`types.rs:113`) for bare
  identifiers, `types::format_call` (`types.rs:45`) for applied ones.
- **Add an output format** → new file under `src/generate/`, re-export from
  `generate/mod.rs:12`, add an `OutputFormat` variant (`lib.rs:26`) and a matching
  `generate_doc` arm (`lib.rs:568`).
- **Add a CLI flag** → the relevant flattened `Args` struct in `lib.rs`, then wire it into
  `filter_options` or `generate_doc`. The README's flag table needs a matching row.
- **Change HTML appearance** → `generate/html/template.rs` (CSS/JS) or
  `generate/html/render.rs` (per-option markup).

## Fork notes

Hard fork of `Thunderbottom/nix-options-doc`. Only `origin` is configured (this fork); there is
no upstream remote and no upstream-compatibility constraint on changes.

`Cargo.toml` still has `publish = false` and the original author/repository metadata, and
`.github/workflows/build-release.yml` still points at the upstream release flow — worth
revisiting before cutting any release from this fork.
