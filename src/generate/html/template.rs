//! Static HTML/CSS/JS scaffolding for the generated document: the
//! `<head>` (styles, no-flash theme restore script) and the instant
//! search/filter script. Per-option markup lives in [`super::render`].

/// Instant client-side regex search, plus click-to-filter by type
/// category, over the rendered options.
///
/// `__SEARCH_INDEX__` is substituted with a JSON array of per-option
/// searchable text (name, description, type, default, example), in the
/// same order as the `.option` elements in the document; category
/// filtering reads the `data-category` attribute already present on
/// each `.option` element directly, so it needs no separate index.
pub(super) const SEARCH_SCRIPT_TEMPLATE: &str = r#"    <script>
    (function () {
        const searchText = __SEARCH_INDEX__;
        const input = document.getElementById('search-input');
        const status = document.getElementById('search-status');
        const options = document.querySelectorAll('.option');
        const legendButtons = document.querySelectorAll('.legend-chip');
        const themeToggle = document.getElementById('theme-toggle');
        let activeCategory = null;

        function setThemeLabel(theme) {
            themeToggle.setAttribute(
                'aria-label',
                theme === 'dark' ? 'Switch to light theme' : 'Switch to dark theme'
            );
        }

        setThemeLabel(document.documentElement.getAttribute('data-theme'));

        themeToggle.addEventListener('click', () => {
            const next = document.documentElement.getAttribute('data-theme') === 'dark' ? 'light' : 'dark';
            document.documentElement.setAttribute('data-theme', next);
            setThemeLabel(next);
            try {
                localStorage.setItem('nix-options-doc-theme', next);
            } catch (e) {}
        });

        function runSearch() {
            const query = input.value.trim();
            input.classList.remove('invalid');
            status.classList.remove('invalid');

            let regex = null;
            if (query !== '') {
                try {
                    regex = new RegExp(query, 'i');
                } catch (e) {
                    input.classList.add('invalid');
                    status.classList.add('invalid');
                    status.textContent = 'Invalid regular expression';
                    return;
                }
            }

            let visible = 0;
            options.forEach((el, i) => {
                const matchesQuery = !regex || regex.test(searchText[i]);
                const matchesCategory = !activeCategory || el.dataset.category === activeCategory;
                const matches = matchesQuery && matchesCategory;
                el.classList.toggle('search-hidden', !matches);
                if (matches) visible++;
            });

            status.textContent = (regex || activeCategory)
                ? `Showing ${visible} of ${options.length} options`
                : '';
        }

        input.addEventListener('input', runSearch);

        legendButtons.forEach((btn) => {
            btn.addEventListener('click', () => {
                const category = btn.dataset.category;
                activeCategory = activeCategory === category ? null : category;
                legendButtons.forEach((b) => {
                    b.classList.toggle('active', b.dataset.category === activeCategory);
                });
                runSearch();
            });
        });

        document.addEventListener('keydown', (e) => {
            if (e.key === '/' && document.activeElement !== input) {
                e.preventDefault();
                input.focus();
            }
        });
    })();
    </script>
"#;

pub(super) const HTML_TEMPLATE_HEAD: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <meta name="color-scheme" content="light dark">
    <title>NixOS Module Options</title>
    <script>
        // Restore a saved theme choice before first paint, so there is
        // no flash of the wrong theme when it differs from the system
        // preference. Falls back to the system preference otherwise -
        // see the `@media (prefers-color-scheme: dark)` rule below.
        (function () {
            try {
                var stored = localStorage.getItem('nix-options-doc-theme');
                var theme = stored || (window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light');
                document.documentElement.setAttribute('data-theme', theme);
            } catch (e) {}
        })();
    </script>
    <style>
        :root {
            --bg: #F6F9FB;
            --surface: #FFFFFF;
            --surface-2: #EDF2F6;
            --ink: #12202B;
            --ink-muted: #5B6B78;
            --line: #D8E2E8;
            --accent: #1C6E8C;
            --danger: #B3261E;

            --c-bool: #B8720A;
            --c-choice: #6B7F1F;
            --c-string: #2563A8;
            --c-number: #1F7A4D;
            --c-package: #A63D8F;
            --c-list: #0E7C7B;
            --c-set: #7C3AED;
            --c-submodule: #B23A6B;
            --c-any: #5B6B78;
        }

        @media (prefers-color-scheme: dark) {
            :root {
                --bg: #0D1216;
                --surface: #141B21;
                --surface-2: #1B242B;
                --ink: #E7EEF2;
                --ink-muted: #8CA0AC;
                --line: #263139;
                --accent: #5FB8DE;
                --danger: #E5847C;

                --c-bool: #E3A339;
                --c-choice: #A8C24B;
                --c-string: #6FAEEA;
                --c-number: #5FC98A;
                --c-package: #D97FC9;
                --c-list: #4FC9C0;
                --c-set: #B18AF5;
                --c-submodule: #E187AE;
                --c-any: #93A4AF;
            }
        }

        /* Explicit choice (via the theme toggle) always wins over the
           system preference above - same values, just applied via an
           attribute selector instead of a media query. */
        :root[data-theme="dark"] {
            --bg: #0D1216;
            --surface: #141B21;
            --surface-2: #1B242B;
            --ink: #E7EEF2;
            --ink-muted: #8CA0AC;
            --line: #263139;
            --accent: #5FB8DE;
            --danger: #E5847C;

            --c-bool: #E3A339;
            --c-choice: #A8C24B;
            --c-string: #6FAEEA;
            --c-number: #5FC98A;
            --c-package: #D97FC9;
            --c-list: #4FC9C0;
            --c-set: #B18AF5;
            --c-submodule: #E187AE;
            --c-any: #93A4AF;
        }
        :root[data-theme="light"] {
            --bg: #F6F9FB;
            --surface: #FFFFFF;
            --surface-2: #EDF2F6;
            --ink: #12202B;
            --ink-muted: #5B6B78;
            --line: #D8E2E8;
            --accent: #1C6E8C;
            --danger: #B3261E;

            --c-bool: #B8720A;
            --c-choice: #6B7F1F;
            --c-string: #2563A8;
            --c-number: #1F7A4D;
            --c-package: #A63D8F;
            --c-list: #0E7C7B;
            --c-set: #7C3AED;
            --c-submodule: #B23A6B;
            --c-any: #5B6B78;
        }

        @media (prefers-reduced-motion: reduce) {
            * { transition-duration: 0.01ms !important; animation-duration: 0.01ms !important; }
        }

        * { box-sizing: border-box; }

        body {
            font-family: ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif;
            margin: 0 auto;
            max-width: 840px;
            padding: 2.5em 1.25em 4em;
            line-height: 1.6;
            color: var(--ink);
            background: var(--bg);
        }

        a { color: var(--accent); text-decoration: none; }
        a:hover { text-decoration: underline; }

        a:focus-visible, button:focus-visible, input:focus-visible {
            outline: 2px solid var(--accent);
            outline-offset: 2px;
            border-radius: 2px;
        }

        code {
            font-family: ui-monospace, "SF Mono", "Cascadia Code", "Fira Code", Consolas, monospace;
            background: var(--surface-2);
            padding: 0.15em 0.4em;
            border-radius: 4px;
            font-size: 0.9em;
        }

        pre {
            background: var(--surface-2);
            border: 1px solid var(--line);
            border-radius: 6px;
            padding: 0.9em 1em;
            margin: 0;
            overflow: auto;
        }
        pre code { background: transparent; padding: 0; font-size: 0.85em; }

        /* ---- masthead ---- */

        .masthead-top {
            display: flex;
            align-items: baseline;
            justify-content: space-between;
            gap: 1em;
        }
        .eyebrow {
            font-family: ui-monospace, "SF Mono", "Cascadia Code", monospace;
            font-size: 0.75em;
            font-weight: 600;
            letter-spacing: 0.12em;
            text-transform: uppercase;
            color: var(--ink-muted);
            margin: 0;
        }
        .opt-count {
            font-family: ui-monospace, "SF Mono", "Cascadia Code", monospace;
            font-size: 0.8em;
            color: var(--ink-muted);
            margin: 0;
        }
        .opt-count strong { color: var(--ink); font-weight: 600; }

        .masthead-right {
            display: flex;
            align-items: center;
            gap: 0.85em;
        }
        .theme-toggle {
            display: inline-flex;
            align-items: center;
            justify-content: center;
            width: 26px;
            height: 26px;
            padding: 0;
            border: 1px solid var(--line);
            border-radius: 6px;
            background: var(--surface);
            color: var(--ink-muted);
            cursor: pointer;
            transition: color 120ms ease, border-color 120ms ease;
        }
        .theme-toggle:hover { color: var(--ink); border-color: var(--accent); }
        .theme-toggle svg { width: 14px; height: 14px; }
        .theme-toggle .icon-sun { display: none; }
        :root[data-theme="dark"] .theme-toggle .icon-sun { display: block; }
        :root[data-theme="dark"] .theme-toggle .icon-moon { display: none; }

        /* ---- toolbar (sticky) ---- */

        .toolbar {
            position: sticky;
            top: 0;
            z-index: 1;
            background: var(--bg);
            padding: 0.5em 0 1em;
            margin-bottom: 1em;
            border-bottom: 1px solid var(--line);
        }

        .search-row { position: relative; margin-top: 0.75em; }
        .search-icon {
            position: absolute;
            left: 0.75em;
            top: 50%;
            transform: translateY(-50%);
            width: 15px;
            height: 15px;
            color: var(--ink-muted);
            pointer-events: none;
        }
        #search-input {
            width: 100%;
            font: inherit;
            font-family: ui-monospace, "SF Mono", "Cascadia Code", monospace;
            font-size: 0.9em;
            color: var(--ink);
            background: var(--surface);
            padding: 0.65em 2.5em 0.65em 2.25em;
            border: 1px solid var(--line);
            border-radius: 6px;
            outline: none;
            transition: border-color 120ms ease, box-shadow 120ms ease;
        }
        #search-input::placeholder { color: var(--ink-muted); }
        #search-input:focus {
            border-color: var(--accent);
            box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 18%, transparent);
        }
        #search-input.invalid {
            border-color: var(--danger);
            box-shadow: 0 0 0 3px color-mix(in srgb, var(--danger) 18%, transparent);
        }
        .search-kbd {
            position: absolute;
            right: 0.6em;
            top: 50%;
            transform: translateY(-50%);
            font-family: ui-monospace, "SF Mono", "Cascadia Code", monospace;
            font-size: 0.75em;
            color: var(--ink-muted);
            border: 1px solid var(--line);
            border-radius: 4px;
            padding: 0.05em 0.4em;
            pointer-events: none;
        }
        #search-status {
            margin-top: 0.5em;
            min-height: 1.2em;
            font-family: ui-monospace, "SF Mono", "Cascadia Code", monospace;
            font-size: 0.75em;
            color: var(--ink-muted);
        }
        #search-status.invalid { color: var(--danger); }

        .legend {
            display: flex;
            flex-wrap: wrap;
            gap: 0.4em;
            margin-top: 0.75em;
        }

        /* ---- type badges + legend chips share a palette ---- */

        .type-badge, .legend-chip {
            --c: var(--c-any);
            display: inline-flex;
            align-items: center;
            font-family: ui-monospace, "SF Mono", "Cascadia Code", monospace;
            font-size: 0.72em;
            font-weight: 600;
            letter-spacing: 0.02em;
            color: var(--c);
            background: color-mix(in srgb, var(--c) 14%, var(--surface));
            border: 1px solid color-mix(in srgb, var(--c) 32%, transparent);
            border-radius: 4px;
            padding: 0.2em 0.55em;
            white-space: nowrap;
        }
        .legend-chip {
            font-family: ui-monospace, "SF Mono", "Cascadia Code", monospace;
            cursor: pointer;
            background: var(--surface);
            transition: background-color 120ms ease, color 120ms ease;
        }
        .legend-chip:hover { background: color-mix(in srgb, var(--c) 10%, var(--surface)); }
        .legend-chip.active {
            color: var(--surface);
            background: var(--c);
            border-color: var(--c);
        }

        .t-bool { --c: var(--c-bool); }
        .t-choice { --c: var(--c-choice); }
        .t-string { --c: var(--c-string); }
        .t-number { --c: var(--c-number); }
        .t-package { --c: var(--c-package); }
        .t-list { --c: var(--c-list); }
        .t-set { --c: var(--c-set); }
        .t-submodule { --c: var(--c-submodule); }
        .t-any { --c: var(--c-any); }

        /* ---- option entries (ledger style: hairlines, not boxes) ---- */

        .option {
            padding: 1.75em 0;
            border-bottom: 1px solid var(--line);
        }
        .option.search-hidden { display: none; }
        .option:first-of-type { padding-top: 0.5em; }
        .option:last-of-type { border-bottom: none; }

        .option-head {
            display: flex;
            align-items: baseline;
            justify-content: space-between;
            gap: 1em;
            flex-wrap: wrap;
        }

        .option-path {
            font-family: ui-monospace, "SF Mono", "Cascadia Code", monospace;
            font-size: 1.05em;
            font-weight: 600;
            margin: 0;
            word-break: break-word;
        }
        .option-path a { color: inherit; }
        .path-prefix { color: var(--ink-muted); font-weight: 400; }
        .path-leaf { color: var(--ink); }

        .option-desc {
            margin: 0.6em 0 0;
            color: var(--ink);
        }
        .option-desc p:first-child { margin-top: 0; }
        .option-desc p:last-child { margin-bottom: 0; }

        .option-meta {
            display: flex;
            flex-wrap: wrap;
            gap: 0.5em 1.5em;
            margin: 0.9em 0 0;
        }
        .meta-row { display: flex; align-items: baseline; gap: 0.5em; }
        .meta-row.block { flex-direction: column; align-items: stretch; flex-basis: 100%; gap: 0.3em; }
        .meta-label {
            font-family: ui-monospace, "SF Mono", "Cascadia Code", monospace;
            font-size: 0.72em;
            font-weight: 600;
            letter-spacing: 0.06em;
            text-transform: uppercase;
            color: var(--ink-muted);
        }

        .option-decl {
            margin-top: 0.9em;
            font-family: ui-monospace, "SF Mono", "Cascadia Code", monospace;
            font-size: 0.78em;
            color: var(--ink-muted);
        }
        .also-declared { margin-top: 0.4em; padding-left: 1em; list-style: none; }
        .also-declared li { margin-top: 0.3em; }
        .also-declared .alt-desc { color: var(--ink-muted); margin: 0.2em 0 0; }

        /* ---- github-alert admonitions inside descriptions ---- */

        .markdown-alert {
            padding: 0.6em 1em;
            margin: 0.75em 0;
            border-radius: 6px;
            border-left: 3px solid var(--line);
            background: var(--surface-2);
        }
        .markdown-alert p { margin: 0.4em 0; }
        .markdown-alert-title {
            font-weight: 700;
            font-size: 0.85em;
            letter-spacing: 0.03em;
            text-transform: uppercase;
            margin-bottom: 0.3em !important;
        }
        .markdown-alert-note { border-left-color: #1F6FEB; }
        .markdown-alert-tip { border-left-color: #2DA44E; }
        .markdown-alert-important { border-left-color: #8250DF; }
        .markdown-alert-warning { border-left-color: #9A6700; }
        .markdown-alert-caution { border-left-color: #CF222E; }

        .footer {
            margin-top: 3em;
            padding-top: 1.5em;
            border-top: 1px solid var(--line);
            font-family: ui-monospace, "SF Mono", "Cascadia Code", monospace;
            font-size: 0.78em;
            color: var(--ink-muted);
        }

        @media (max-width: 520px) {
            .search-kbd { display: none; }
        }
    </style>
</head>
<body>
"#;
