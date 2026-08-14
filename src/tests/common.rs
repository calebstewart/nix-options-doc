use std::fs;
use std::path::Path;

/// Creates a test file with the specified filename and content in the given directory.
///
/// # Arguments
/// - `dir`: The directory in which to create the file.
/// - `filename`: The name of the file to create.
/// - `content`: The content to write into the file.
///
/// # Returns
/// A Result indicating success or an I/O error.
pub(super) fn create_test_file(
    dir: &Path,
    filename: &str,
    content: &str,
) -> Result<(), std::io::Error> {
    fs::write(dir.join(filename), content)
}
