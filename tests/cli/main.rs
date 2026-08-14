//! Process-level CLI tests for #9: warnings are invisible by default, and
//! the "no options found" / "no options match the filters" paths used to
//! `return Ok(())` before ever calling `generate_doc`, so `--out` was left
//! untouched (and, on repeat runs, a stale file from an earlier invocation
//! silently persisted).
//!
//! These behaviors only exist at the level of `main` - process exit status,
//! whether `--out` gets written, and default log visibility - which the
//! unit tests under `src/tests/` cannot observe because they call library
//! functions directly and never spawn the binary. Hence this is a normal
//! integration test target that builds and runs the actual
//! `nix-options-doc` binary via `env!("CARGO_BIN_EXE_nix-options-doc")`.
//!
//! The target is split by area, one file per module below, so concurrent
//! branches adding CLI tests do not all append to a single file and collide
//! on rebase (#54) - the same reasoning that split the unit-test tree across
//! `src/tests/` (#37). `common` holds the shared helpers used by the area
//! modules.

mod common;
// `fs_errors` is gated at the *module* level, not per function: every test in it
// is Unix-only, and an ungated module would leave its `use` items unused on
// Windows, which CI's RUSTFLAGS='-Dwarnings' turns into a hard error. A new
// non-Unix CLI test therefore does not belong in this module.
#[cfg(unix)]
mod fs_errors;
mod help;
mod logging;
mod output;
mod path_errors;
