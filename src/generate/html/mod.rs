mod render;
mod template;

use crate::error::NixDocError;
use crate::OptionDoc;
use comrak::Options as ComrakOptions;
use render::CATEGORIES;
use std::collections::HashSet;
use template::{HTML_TEMPLATE_HEAD, SEARCH_SCRIPT_TEMPLATE};

/// Generates an HTML document containing comprehensive documentation for NixOS module options.
///
/// # Arguments
/// - `options`: A slice of option documentation entries to render as HTML.
///
/// # Returns
/// A `Result` containing the complete HTML document with styling and navigation or an error.
pub fn generate_html(options: &[OptionDoc]) -> Result<String, NixDocError> {
    let mut output = String::with_capacity(options.len() * 800 + 1500);
    output.push_str(HTML_TEMPLATE_HEAD);

    // Set up markdown rendering options
    let mut comrak_options = ComrakOptions::default();
    comrak_options.extension.strikethrough = true;
    comrak_options.extension.table = true;
    comrak_options.extension.autolink = true;
    comrak_options.extension.tasklist = true;
    comrak_options.extension.alerts = true;
    comrak_options.render.r#unsafe = true; // Allow HTML in markdown (if needed)

    let mut categories_present = HashSet::new();

    // Per-option search text and category, parallel to the `.option`
    // elements below - handed to the search Worker, which has no DOM access.
    let mut search_index: Vec<String> = Vec::with_capacity(options.len());
    let mut category_index: Vec<&'static str> = Vec::with_capacity(options.len());
    let mut body = String::with_capacity(options.len() * 700);

    for option in options {
        search_index.push(render::search_index_entry(option));

        let (article, category) = render::render_option(option, &comrak_options);
        category_index.push(category.0);
        categories_present.insert(category);
        body.push_str(&article);
    }

    let mut legend = String::new();
    for (class, label) in CATEGORIES {
        if categories_present.contains(&(class, label)) {
            legend.push_str(&format!(
                r#"            <button type="button" class="legend-chip t-{class}" data-category="{class}">{label}</button>
"#
            ));
        }
    }

    output.push_str(&format!(
        r#"    <div class="masthead-top">
        <h1 class="eyebrow">Nix Options</h1>
        <div class="masthead-right">
            <p class="opt-count"><strong>{count}</strong> option{plural}</p>
            <button type="button" id="theme-toggle" class="theme-toggle" aria-label="Switch to dark theme">
                <svg class="icon-sun" viewBox="0 0 16 16" fill="none" aria-hidden="true">
                    <circle cx="8" cy="8" r="3" stroke="currentColor" stroke-width="1.3"/>
                    <path d="M8 1v2M8 13v2M1 8h2M13 8h2M3.5 3.5l1.4 1.4M11.1 11.1l1.4 1.4M3.5 12.5l1.4-1.4M11.1 4.9l1.4-1.4" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/>
                </svg>
                <svg class="icon-moon" viewBox="0 0 16 16" fill="none" aria-hidden="true">
                    <path d="M13.5 9.5A5.5 5.5 0 1 1 6.5 2.5a4.25 4.25 0 1 0 7 7Z" fill="currentColor"/>
                </svg>
            </button>
        </div>
    </div>
    <div class="toolbar">
        <div class="search-row">
            <svg class="search-icon" viewBox="0 0 16 16" fill="none" aria-hidden="true">
                <circle cx="7" cy="7" r="5.25" stroke="currentColor" stroke-width="1.5"/>
                <path d="M11 11L14.5 14.5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
            </svg>
            <input type="text" id="search-input" placeholder="Search options" autocomplete="off" aria-label="Search options">
            <kbd class="search-kbd">/</kbd>
        </div>
        <div id="search-status" role="status"></div>
        <div class="legend" role="group" aria-label="Filter by type">
{legend}        </div>
    </div>
"#,
        count = options.len(),
        plural = if options.len() == 1 { "" } else { "s" },
    ));

    output.push_str(&body);

    // Inject the instant search script with its per-option search index.
    // The `</` guard prevents a description or example containing that
    // literal text from prematurely closing the <script> tag.
    let search_index_json = serde_json::to_string(&search_index)
        .map_err(|e| NixDocError::Serialization(e.to_string()))?
        .replace("</", "<\\/");
    let category_index_json = serde_json::to_string(&category_index)
        .map_err(|e| NixDocError::Serialization(e.to_string()))?
        .replace("</", "<\\/");
    output.push_str(
        &SEARCH_SCRIPT_TEMPLATE
            .replace("__SEARCH_INDEX__", &search_index_json)
            .replace("__CATEGORY_INDEX__", &category_index_json),
    );

    output.push_str(&format!(
        r#"    <p class="footer">generated with <a href="{}">{}</a></p>
</body>
</html>"#,
        option_env!("CARGO_PKG_REPOSITORY").unwrap_or(env!("CARGO_PKG_NAME")),
        env!("CARGO_PKG_NAME")
    ));

    Ok(output)
}
