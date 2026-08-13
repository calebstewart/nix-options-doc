//! Unit tests for the crate, split by the subsystem each group exercises.
//!
//! This module is the crate's `#[cfg(test)] mod tests`, so `use super::*`
//! below reaches the crate root's own (private) imports and items; each area
//! file re-globs it with a plain `use super::*;` and therefore sees exactly
//! the same names. Add a new test to the area file that matches the code it
//! exercises - that is what keeps concurrent branches from colliding on one
//! shared file (see issue #37).

use super::*;
use crate::generate::generate_html;
use crate::generate::generate_markdown;
use common::create_test_file;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

mod cli;
mod collect;
mod common;
mod filter;
mod generate;
mod metadata;
mod parser;
mod types;
mod utils;
