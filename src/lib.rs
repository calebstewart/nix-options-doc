pub mod error;
pub mod generate;
pub mod nix_call;
pub mod parser;
pub mod types;
pub mod utils;

use crate::error::NixDocError;
use clap::{ArgGroup, Args, Parser};
use gix::{progress::Discard, remote::fetch::Shallow};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use tempfile::TempDir;
use walkdir::WalkDir;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum OutputFormat {
    Markdown,
    Json,
    Html,
    Csv,
}

/// Command-line interface configuration and options.
///
/// Contains all command-line arguments grouped by functionality.
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// Generate a shell completion script for the given shell and exit
    #[arg(long, value_enum, exclusive = true)]
    pub generate_completions: Option<clap_complete::Shell>,

    #[command(flatten)]
    pub io: IoOptions,

    #[command(flatten)]
    pub git: GitOptions,

    #[command(flatten)]
    pub filter: FilterOptions,

    #[command(flatten)]
    pub util: UtilityOptions,
}

/// Input/output related command options.
///
/// Controls where to read Nix files from and how to output documentation.
#[derive(Args)]
#[command(group(ArgGroup::new("io")))]
pub struct IoOptions {
    /// Local path or remote git repository URL to the nix configuration
    #[arg(short, long, default_value = ".")]
    pub path: String,

    /// Path to the output file or 'stdout'
    #[arg(short, long, default_value = "stdout")]
    pub out: String,

    /// Output format
    #[arg(short = 'f', long, default_value = "markdown")]
    pub format: OutputFormat,

    /// Whether the output should be sorted (asc.)
    #[arg(short, long)]
    pub sort: bool,

    /// Prefix path or URL for the output options
    #[arg(long, value_name = "PATH")]
    pub out_prefix: Option<String>,
}

/// Git repository related command options.
///
/// Controls how to fetch and use Git repositories.
#[derive(Args)]
#[command(group(ArgGroup::new("git")))]
pub struct GitOptions {
    /// Git branch or tag to use (if repository URL provided)
    #[arg(short, long)]
    pub branch: Option<String>,

    /// Git commit depth (set to 1 for shallow clone)
    #[arg(short, long, default_value = "1")]
    pub depth: u32,
}

/// Options for filtering and modifying the documentation output.
///
/// Controls which options to include and how to format them.
#[derive(Args)]
#[command(group(ArgGroup::new("filter")))]
pub struct FilterOptions {
    /// Filter options by prefix (e.g. "services.nginx")
    #[arg(long, value_name = "PREFIX")]
    pub filter_by_prefix: Option<String>,

    /// Filter options by type (e.g. "bool", "string")
    #[arg(long, value_name = "NIX_TYPE")]
    pub filter_by_type: Option<String>,

    /// Search in option names and descriptions
    #[arg(long, value_name = "OPTION")]
    pub search: Option<String>,

    /// Only show options that have a default value
    #[arg(long)]
    pub has_default: bool,

    /// Only show options that have a description
    #[arg(long)]
    pub has_description: bool,

    /// Replace nix variables in the generated
    /// document with the specified value
    /// (can be used multiple times)
    #[arg(long, value_parser = utils::parse_key_value)]
    #[arg(value_name = "KEY=VALUE")]
    pub replace: Vec<(String, String)>,

    /// Remove the specified prefix from option names in
    /// the generated documentation. A prefix that does not
    /// already start with `options.` is treated as
    /// `options.<PREFIX>`, and a missing trailing dot is
    /// added. Given without a value, strips `options.`.
    #[arg(long, value_name = "PREFIX")]
    #[arg(num_args = 0..=1, default_missing_value = "options.")]
    pub strip_prefix: Option<String>,
}

/// Utility options for controlling the documentation process.
///
/// Controls progress display and file traversal behavior.
#[derive(Args)]
#[command(group(ArgGroup::new("utility")))]
pub struct UtilityOptions {
    /// Directories to exclude from processing (can be specified multiple times)
    #[arg(short = 'e', long, value_delimiter = ',')]
    pub exclude_dir: Vec<String>,

    /// Enable traversing through symbolic links
    ///
    /// Hidden-directory pruning applies to the tree being walked, not to the
    /// targets links resolve to: a non-hidden symlink that points into a
    /// hidden directory is followed, and the options it reaches are
    /// documented. Scanning an untrusted tree with this flag can therefore
    /// read files the visible tree does not appear to expose.
    #[arg(long)]
    pub follow_symlinks: bool,

    /// Show progress bar
    #[arg(long)]
    pub progress: bool,
}

/// A single source location where an option is declared.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Declaration {
    /// The relative path to the file where the option is defined
    pub file_path: String,

    /// The line number where the option is defined in the file
    pub line_number: usize,

    /// This declaration's own description, populated only when an option
    /// is declared more than once and this particular declaration's
    /// description differs from the option's primary (first-found) one.
    pub description: Option<String>,

    /// The `mkIf` condition(s) this declaration is guarded by, if any
    /// (joined with `&&` for nested `mkIf`s). This is the condition
    /// expression's source text, not an evaluated result.
    pub condition: Option<String>,
}

/// Represents a documented NixOS module option.
///
/// Contains all metadata about a single option including its name,
/// type, description, default value, and source location(s). An option
/// may be declared more than once (e.g. re-declared across module
/// fragments), so declarations is a list rather than a single location.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OptionDoc {
    /// The full name of the option with dot notation
    pub name: String,

    /// The description text explaining the option's purpose and usage
    pub description: Option<String>,

    /// The type of the option (bool, string, int, etc.)
    pub nix_type: String,

    /// The default value of the option, if any
    pub default_value: Option<String>,

    /// An example value for the option, if provided
    pub example: Option<String>,

    /// For a `mkRenamedOptionModule` shim entry, the bare config path
    /// (no `options.` prefix) of the option it was renamed to. `None`
    /// for every other option. Kept separate from `description` (which
    /// already mentions this in prose) so the new option's anchor link
    /// can be resolved once `--strip-prefix` is known (see
    /// `filter_options`), rather than baked in at parse time when it
    /// isn't yet.
    pub renamed_to: Option<String>,

    /// The source location(s) where this option is declared
    pub declarations: Vec<Declaration>,
}

/// Filters the list of option documentation entries based on CLI parameters.
///
/// # Arguments
/// - `options`: A slice of option documentation entries to filter.
/// - `cli`: The CLI arguments containing filter criteria (prefix, type, search term, etc.).
///
/// # Returns
/// A vector of options that match all specified filter conditions.
pub fn filter_options(options: &[OptionDoc], cli: &Cli) -> Vec<OptionDoc> {
    let mut filtered = options.to_vec();

    // Filter by prefix
    if let Some(ref prefix) = cli.filter.filter_by_prefix {
        filtered.retain(|opt| opt.name.starts_with(prefix));
    }

    // Filter by type
    if let Some(ref type_str) = cli.filter.filter_by_type {
        filtered.retain(|opt| {
            let type_info = opt.nix_type.to_lowercase();
            type_info.contains(&type_str.to_lowercase())
        });
    }

    // Filter by search text
    if let Some(ref search) = cli.filter.search {
        match regex::Regex::new(search) {
            Ok(re) => {
                filtered.retain(|opt| {
                    re.is_match(&opt.name)
                        || opt.description.as_ref().is_some_and(|d| re.is_match(d))
                });
            }
            Err(e) => {
                // Log the error but don't filter out anything if the regex is invalid
                log::error!("Invalid regex pattern '{}': {}", search, e);
            }
        }
    }

    // Filter by having default value
    if cli.filter.has_default {
        filtered.retain(|opt| opt.default_value.is_some());
    }

    // Filter by having description
    if cli.filter.has_description {
        filtered.retain(|opt| opt.description.is_some());
    }

    // Strip prefix: `options.*`
    let strip_prefix_pattern = cli.filter.strip_prefix.as_ref().map(|strip_prefix| {
        // Qualify first, on the *raw* value: a prefix that doesn't already
        // start with `options.` is documented to mean `options.<PREFIX>`.
        // This has to happen before any trailing-dot trimming - trimming
        // first would turn the already-qualified `options.` (the flag's own
        // no-value default) into a bare-looking `options`, which would then
        // be re-qualified to `options.options.`.
        let qualified = if strip_prefix.starts_with("options.") {
            strip_prefix.clone()
        } else {
            format!("options.{strip_prefix}")
        };
        // Then normalize the trailing dot exactly once, whichever branch
        // produced the value. Appending unconditionally is what made a
        // bare prefix that already ended in a dot (`services.foo.`) come
        // out as `options.services.foo..`, a pattern no option name can
        // start with - so the flag silently stripped nothing (#40).
        format!("{}.", qualified.trim_end_matches('.'))
    });

    if let Some(prefix) = &strip_prefix_pattern {
        log::debug!("Stripping prefix `{}` from the generated document", prefix);

        for opt in &mut filtered {
            // Only a *leading* match is a prefix. `str::replace` would also
            // strip mid-name occurrences (a nested `options.services.` inside
            // a submodule path, say), mangling names that merely contain the
            // pattern - see nix-options-doc#2.
            if let Some(rest) = opt.name.strip_prefix(prefix) {
                opt.name = rest.to_string();
            }
        }
    }

    // A `mkRenamedOptionModule` shim's description mentions the new
    // option by its bare config path (e.g. "Use `services.newName`
    // instead."), left unlinked at parse time since the new option's
    // actual anchor depends on whatever --strip-prefix ends up doing to
    // it, and that isn't known until here. Resolve it now, using the
    // exact same stripping that was just applied to every real option's
    // name, so the two stay consistent regardless of whether
    // --strip-prefix was used.
    for opt in &mut filtered {
        let Some(target) = opt.renamed_to.clone() else {
            continue;
        };
        let mut target_name = format!("options.{target}");
        if let Some(prefix) = &strip_prefix_pattern {
            if let Some(rest) = target_name.strip_prefix(prefix) {
                target_name = rest.to_string();
            }
        }
        let anchor = utils::anchor_slug(&target_name);
        if let Some(description) = &mut opt.description {
            // `bare` must reproduce, byte for byte, the span `parser::find_deprecations`
            // wrote into this description - the delimiter length and padding depend on
            // the target's own content (issue #49), so both sides derive it from the
            // same `inline_code` call on the same string rather than hard-coding a
            // single backtick.
            let bare = utils::inline_code(&target);
            let linked = format!("[{bare}](#{anchor})");
            *description = description.replacen(&bare, &linked, 1);
        }
    }

    if let Some(out_prefix) = &cli.io.out_prefix {
        let prefix = if out_prefix.ends_with('/') {
            out_prefix.strip_suffix('/').unwrap_or(out_prefix.as_str())
        } else {
            out_prefix.as_str()
        };

        for opt in &mut filtered {
            for decl in &mut opt.declarations {
                decl.file_path = format!("{}/{}", prefix, decl.file_path);
            }
        }
    }

    filtered
}

/// Prepares a local directory for processing Nix files.
///
/// # Arguments
/// - `cli`: The CLI arguments containing path, branch, depth, and other repository options.
///
/// # Returns
/// A tuple containing the path to the working directory and an optional `TempDir` (for cleanup).
/// If the path is local, returns the local path with None for `TempDir`.
/// If the path is a git URL, clones the repository and returns the temp directory.
///
/// A local path that does not exist is reported as `NixDocError::LocalPathNotFound`
/// rather than being handed to the clone branch: a value is only treated as a git
/// URL when it is not an absolute filesystem path *and* `gix` parses it with a
/// non-`file` scheme (`https`, `http`, `ssh`, `git`), or the value is an explicit
/// `file://` URL (which still clones, so a caller can pin a branch of a local
/// repository).
pub fn prepare_path(cli: &Cli) -> Result<(PathBuf, Option<TempDir>), NixDocError> {
    // Check if the path is a local directory
    let path = Path::new(&cli.io.path);
    match path.try_exists() {
        Ok(true) => {
            log::debug!("Found local path: {}", path.to_string_lossy());
            return Ok((path.to_path_buf(), None));
        }
        // `try_exists` rather than `exists`: `exists()` maps every IO error
        // (notably an unreadable parent directory) to `false`, and reporting
        // "does not exist" for a path that does exist would be the same
        // class of misleading diagnostic this branch was added to fix
        // (#52). This arm only catches a genuine permission failure, though
        // - see the fallthrough arm below for why.
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            return Err(NixDocError::LocalPathUnreadable(
                cli.io.path.clone(),
                e.to_string(),
            ));
        }
        // Either the path genuinely does not exist (`Ok(false)`), or - on
        // Windows only - the value isn't even syntactically valid path
        // syntax, which `try_exists` reports as `Err(ERROR_INVALID_NAME)`
        // rather than `Ok(false)`. A URL-shaped value such as
        // `https://example.invalid/o/r.git` hits exactly this case: it is
        // not a valid Windows path, so treating every `Err` as "exists but
        // unreadable" (the arm above) would report an access failure -
        // before the URL test below ever runs - for *every* remote clone on
        // Windows. Both cases therefore fall through to the "is this a git
        // URL?" test below.
        Ok(false) | Err(_) => {}
    }

    // The path does not exist (or isn't valid path syntax at all), so this is
    // either a git URL or a mistyped local path. `gix::url::parse` cannot
    // make that call on its own: it happily parses `./modules`,
    // `modules-typo` and `..` as `Scheme::File`, which is exactly why a typo
    // used to fall through to the clone branch and surface as "Failed to
    // clone repository" (#52).
    //
    // The absolute-path test comes first and is load-bearing on Windows:
    // `gix` parses `C:\Users\me\modules` as an *ssh* URL with host "c", so the
    // scheme test alone would still send a Windows path to the clone branch.
    if path.is_absolute() {
        return Err(NixDocError::LocalPathNotFound(cli.io.path.clone()));
    }

    let url = gix::url::parse(cli.io.path.as_bytes())
        .map_err(|e| NixDocError::InvalidPath(format!("Invalid git URL: {}", e)))?;

    // `Scheme::File` covers two different inputs, and only one of them means
    // "does not exist": a bare/relative local path like `./modules` or
    // `modules-typo` (no scheme in the text at all - this is the #52 typo
    // case), versus an *explicit* `file://` URL, which `gix::prepare_clone`
    // supports and which worked on `main`. An existing on-disk path
    // short-circuits above and deliberately ignores `--branch`/`--depth`
    // (see `test_prepare_path_local_path_ignores_branch`), so `file://` is
    // the only way to select a specific branch of a local repository - it
    // must keep taking the clone branch rather than being folded into the
    // "missing local path" case.
    if url.scheme == gix::url::Scheme::File && !cli.io.path.starts_with("file://") {
        return Err(NixDocError::LocalPathNotFound(cli.io.path.clone()));
    }

    let temp_dir = TempDir::new()?;
    let temp_path = temp_dir.path();

    // Attempt to fetch git repository
    // Initialize interrupt handler.
    //
    // `unsafe_code` is denied crate-wide via `[lints.rust]` in `Cargo.toml`;
    // this is the single sanctioned exception. `gix::interrupt::init_handler`
    // is `unsafe` because it installs a process-global signal handler, which
    // is only sound to do once and from a context where no other thread is
    // concurrently installing one. `prepare_path` runs on the main thread
    // before any of the rayon parallelism downstream starts, so that holds.
    #[allow(unsafe_code)]
    unsafe {
        gix::interrupt::init_handler(1, || {}).map_err(|e| {
            NixDocError::GitOperation(format!("Failed to initialize interrupt handler: {}", e))
        })?;
    }

    // Prepare the clone builder
    let mut prepare_clone = gix::prepare_clone(url, temp_path).map_err(|e| {
        let err_msg = e.to_string();
        if err_msg.contains("auth") || err_msg.contains("credentials") {
            NixDocError::GitClone(cli.io.path.clone(), err_msg)
        } else {
            NixDocError::GitOperation(format!("Failed to prepare clone: {}", e))
        }
    })?;

    // Configure shallow clone with the provided depth (defaults to 1)
    let shallow = Shallow::DepthAtRemote(
        std::num::NonZeroU32::new(cli.git.depth)
            .unwrap_or_else(|| std::num::NonZeroU32::new(1).unwrap()),
    );

    // `with_ref_name` validates the ref name locally, before any network
    // round-trip, and `--branch` is arbitrary user input - a typo like
    // "my branch" or "foo..bar" must produce the same clean error as every
    // other git failure in this function, not a panic.
    if let Some(ref branch) = cli.git.branch {
        prepare_clone = prepare_clone.with_ref_name(Some(branch)).map_err(|e| {
            NixDocError::GitOperation(format!("Invalid branch or tag name '{branch}': {e}"))
        })?;
    }
    let (mut prepare_checkout, _) = prepare_clone
        .with_shallow(shallow)
        .fetch_then_checkout(Discard, &gix::interrupt::IS_INTERRUPTED)
        .map_err(|e| NixDocError::GitClone(cli.io.path.clone(), e.to_string()))?;

    let (repo, _) = prepare_checkout
        .main_worktree(Discard, &gix::interrupt::IS_INTERRUPTED)
        .map_err(|e| NixDocError::GitOperation(format!("Failed to checkout worktree: {}", e)))?;

    let work_dir = repo.workdir().ok_or(NixDocError::NoWorkDir)?;
    Ok((work_dir.to_path_buf(), Some(temp_dir)))
}

/// Recursively collects NixOS module options from all .nix files in the specified directory.
///
/// Hidden directories (name starting with `.`, e.g. `.git`, `.direnv`, `.cache`) below the
/// root are pruned during traversal and never descended into; the root itself is exempt from
/// this check, so a hidden directory passed directly as `dir` is still processed.
///
/// That pruning is by name and applies to the tree being walked, not to the targets symbolic
/// links resolve to: with `follow_symlinks` set, a link whose own name is not hidden is
/// followed even when it points into a hidden directory (nix-options-doc#42). That is the
/// specified behavior of the flag, not an oversight - see `utils::should_traverse_entry`.
///
/// # Arguments
/// - `dir`: The base directory to search for Nix files.
/// - `exclude_dirs`: A list of directory paths to exclude from processing.
/// - `replacements`: A map of variable replacements for dynamic parts in option definitions.
/// - `show_progress`: Displays a progress bar if set to true.
/// - `follow_symlinks`: Whether to follow symbolic links during directory traversal.
///
/// # Returns
/// A `Result` containing a vector of unique option documentation entries or an error.
pub fn collect_options(
    dir: &Path,
    exclude_dirs: &[String],
    replacements: &HashMap<String, String>,
    show_progress: bool,
    follow_symlinks: bool,
) -> Result<Vec<OptionDoc>, NixDocError> {
    if !dir.exists() {
        return Err(NixDocError::InvalidPath(format!(
            "Directory does not exist: {}",
            dir.display()
        )));
    }

    if !replacements.is_empty() {
        log::debug!("Using variable replacements:");
        for (key, value) in replacements {
            log::debug!("\t${{{0}}} => {1}", key, value);
        }
    }

    // Collect list of directories and paths to be excluded
    // from the generated documentation
    let exclude_paths: Vec<PathBuf> = exclude_dirs
        .iter()
        .map(|s| {
            let p = PathBuf::from(s);
            if p.is_absolute() {
                p
            } else {
                dir.join(p)
            }
        })
        .collect();

    if !exclude_paths.is_empty() {
        log::debug!("Excluding directories:");
        for path in &exclude_paths {
            log::debug!("\t{}", path.display());
        }
    }

    // Collect all .nix files first
    let mut nix_files = Vec::new();

    // Walk the directory, filtering out excluded paths
    // `filter_entry` rather than filtering in the loop body: when the
    // predicate rejects a directory, `WalkDir` skips the entire subtree
    // instead of descending into it. That is the whole point here - a
    // per-entry check only sees the entry's own name, so a plain
    // `secret.nix` inside `.direnv`/`.git` looks perfectly ordinary and
    // gets documented (see nix-options-doc#8), and `.git` is walked in
    // full on every run.
    for result in WalkDir::new(dir)
        .follow_links(follow_symlinks)
        .into_iter()
        .filter_entry(utils::should_traverse_entry)
    {
        // Handle any errors during directory traversal
        let entry = match result {
            Ok(entry) => entry,
            Err(e) => {
                // `walkdir` reports depth relative to the traversal root, so a
                // depth-0 error is the root itself failing to open - nothing
                // below it can produce one. That is categorically different
                // from a single unreadable subdirectory: nothing was walked at
                // all, so returning `Ok(vec![])` here would report "this tree
                // declares no options" for a tree we never got to look at. The
                // caller then overwrites `--out` with an empty document and
                // exits 0, destroying a good result from an earlier run (#41).
                // Errors below the root stay warnings - partial traversal is
                // still a real result, and graceful degradation there is
                // deliberate.
                if e.depth() == 0 {
                    return Err(NixDocError::WalkDir(e));
                }
                log::warn!("An error occurred, skipping directory: {}", e);
                continue;
            }
        };

        if utils::should_process_file(&entry, &exclude_paths) {
            nix_files.push(entry.path().to_path_buf());
        }
    }

    // `WalkDir` does not guarantee a sorted iteration order (it follows
    // filesystem readdir order), but processing order determines which
    // declaration of a re-declared option is treated as "primary" (see
    // `collect_options`'s merge step below). Sort for a deterministic,
    // reproducible result across runs and machines.
    nix_files.sort();

    // Set up progress bar
    let progress_bar = if show_progress {
        let pb = indicatif::ProgressBar::new(nix_files.len() as u64);
        pb.set_style(
            indicatif::ProgressStyle::default_bar()
                .template(
                    "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})",
                )
                .expect("Invalid progress bar template")
                .progress_chars("#>-"),
        );
        Some(pb)
    } else {
        None
    };

    // Use a thread-safe counter for progress
    let counter = std::sync::atomic::AtomicUsize::new(0);

    // Process files in parallel
    let options: Vec<OptionDoc> = nix_files
        .par_iter()
        .flat_map(|file_path| {
            // Update progress
            if let Some(ref pb) = progress_bar {
                let count = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                pb.set_position(count as u64);
                if let Some(file_name) = file_path.file_name() {
                    pb.set_message(format!("Processing {}", file_name.to_string_lossy()));
                }
            }

            log::debug!(
                "Processing file: {}",
                match file_path.strip_prefix(dir) {
                    Ok(rel) => rel.to_string_lossy(),
                    Err(_) => file_path.to_string_lossy(),
                }
            );

            utils::process_nix_file(file_path, dir, replacements)
        })
        .collect();

    if let Some(pb) = progress_bar {
        pb.finish_with_message("Processing complete");
    }

    log::debug!("Total options found: {}", options.len());

    // Post-process: merge options declared more than once (e.g. the same
    // option name defined in separate module fragments) into a single
    // entry with all of their declarations, rather than silently
    // dropping every declaration after the first.
    let mut unique_options: Vec<OptionDoc> = Vec::new();
    let mut index_by_name: HashMap<String, usize> = HashMap::new();

    for option in options {
        if let Some(&idx) = index_by_name.get(&option.name) {
            // Only carry a per-declaration description when it
            // actually differs from the primary (first-found) one,
            // so callers don't need to repeat it for the common case.
            let alt_description = if unique_options[idx].description != option.description {
                option.description.clone()
            } else {
                None
            };
            for mut decl in option.declarations {
                decl.description = alt_description.clone();
                if !unique_options[idx].declarations.contains(&decl) {
                    unique_options[idx].declarations.push(decl);
                }
            }
        } else {
            index_by_name.insert(option.name.clone(), unique_options.len());
            unique_options.push(option);
        }
    }

    Ok(unique_options)
}

/// Generates documentation for the given options in the specified output format.
///
/// # Arguments
/// - `options`: A slice of option documentation entries to be formatted.
/// - `format`: The desired output format (Markdown, JSON, HTML, or CSV).
/// - `sorted`: If true, sorts the options alphabetically by name.
///
/// # Returns
/// A `Result` containing the generated documentation string in the specified format or an error.
pub fn generate_doc(
    options: &[OptionDoc],
    format: OutputFormat,
    sorted: bool,
) -> Result<String, NixDocError> {
    let mut options_copy = options.to_vec();
    if sorted {
        options_copy.sort_by(|a, b| a.name.cmp(&b.name));
    }

    match format {
        OutputFormat::Markdown => Ok(generate::generate_markdown(&options_copy)?),
        OutputFormat::Json => generate::generate_json(&options_copy),
        OutputFormat::Html => generate::generate_html(&options_copy),
        OutputFormat::Csv => generate::generate_csv(&options_copy),
    }
}
