use clap::{CommandFactory, Parser};
use nix_options_doc::{collect_options, filter_options, generate_doc, prepare_path, Cli};
use std::collections::HashMap;
use std::fs;
use std::io::Write;

/// Runs the application: parses command line arguments, prepares the working directory (or
/// clones a repository), collects NixOS module options from the specified path, applies
/// filtering and variable replacements, generates documentation in the desired format, and
/// outputs the result to stdout or a file.
///
/// # Returns
/// Returns `Ok(())` if the application completes successfully; otherwise returns an error with details.
fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // `env_logger::init()` defaults to the `error` level when `RUST_LOG` is
    // unset, which silently drops every `log::warn!` in the program - including
    // "no options found" and skipped-directory warnings, the two things a user
    // most needs to see. `default_filter_or` supplies "warn" only when `RUST_LOG`
    // is *unset*: env_logger reads the variable first and falls back to the
    // default only when that read fails, so `RUST_LOG=debug`/`RUST_LOG=error`
    // still win. Note `RUST_LOG=""` also wins, and not in the useful direction -
    // an empty value parses to zero filter directives, whereupon env_filter
    // installs its own `error` default and warnings vanish again. That is stock
    // env_logger behaviour and we deliberately do not special-case it; see the
    // `empty_rust_log_falls_back_to_env_loggers_own_error_default` test in
    // tests/cli/logging.rs, which pins it. See #9 and #43.
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let cli = Cli::parse();

    if let Some(shell) = cli.generate_completions {
        let mut cmd = Cli::command();
        let name = cmd.get_name().to_string();
        clap_complete::generate(shell, &mut cmd, name, &mut std::io::stdout());
        return Ok(());
    }

    log::info!("Starting {}", env!("CARGO_PKG_NAME"));
    log::debug!("Input path: {}", cli.io.path);
    log::debug!("Output: {}", cli.io.out);

    let (path, _temp_dir) = prepare_path(&cli)?;

    log::debug!("Using path: {}", path.display());
    log::debug!("Collecting options...");

    // Get replacements for any dynamic variables if defined
    let replacements: HashMap<String, String> = cli.filter.replace.clone().into_iter().collect();
    let options = collect_options(
        &path,
        &cli.util.exclude_dir,
        &replacements,
        cli.util.progress,
        cli.util.follow_symlinks,
    )?;

    // Both "empty" outcomes below are warnings, not errors: a tree with no
    // options is a legitimate thing to point this tool at. But they must still
    // be *observable*, so we fall through and generate the (empty) document
    // rather than returning early. That way `--out` is always written - the
    // previous behaviour left a stale file from an earlier run in place, which
    // a pipeline consuming it could not distinguish from success. The exit code
    // deliberately stays 0; see #9.
    if options.is_empty() {
        log::warn!("No NixOS options found in the specified path");
    }

    // Apply module filters if specified
    let filtered_options = filter_options(&options, &cli);

    // Guarded on `!options.is_empty()` so an empty input tree produces one
    // warning, not two saying the same thing.
    if !options.is_empty() && filtered_options.is_empty() {
        log::warn!(
            "No options match the specified filters (from {} total options)",
            options.len()
        );
    }

    log::debug!("Generating documentation...");

    let output = generate_doc(&filtered_options, cli.io.format, cli.io.sort)?;

    // Output to stdout or file path
    if cli.io.out == "stdout" {
        let stdout = std::io::stdout();
        let mut handle = stdout.lock();

        handle.write_all(output.as_bytes())?;
    } else {
        fs::write(&cli.io.out, &output)?;
        log::info!(
            "Found {} options (filtered from {} total). Documentation generated in: {}",
            filtered_options.len(),
            options.len(),
            cli.io.out
        );
    }

    Ok(())
}

/// Process entry point. Runs [`run`] and turns its error, if any, into a
/// readable message plus a failing exit status.
///
/// Returning `Result` from `main` directly would print the error with
/// `Debug` (that is what the std `Termination` impl does), which for a
/// `thiserror` enum dumps the variant and its whole nested `io::Error`
/// struct instead of the message the `#[error(...)]` attributes define.
/// `std::process::exit` is not an alternative here: `clippy::exit` is
/// enabled in `Cargo.toml`'s `[lints.clippy]` and CI runs clippy with
/// `-Dwarnings`.
fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            // Deliberately not `log::error!`: this is a fatal, user-facing
            // message and must not be silenceable by `RUST_LOG=off`.
            eprintln!("Error: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}
