//! Static HTML/CSS/JS scaffolding for the generated document: the
//! `<head>` (styles, no-flash theme restore script) and the instant
//! search/filter script. Per-option markup lives in [`super::render`].

/// Instant client-side regex search, plus click-to-filter by type
/// category, over the rendered options.
///
/// `__SEARCH_INDEX__` is substituted with a JSON array of per-option
/// searchable text (name, description, type, default, example), in the
/// same order as the `.option` elements in the document. `__CATEGORY_INDEX__`
/// is substituted with a parallel JSON array of each option's type category,
/// used to drive click-to-filter by the legend chips. Both substitutions are
/// single-pass, splitting this pristine template around each placeholder
/// rather than running `String::replace` over already-substituted output, so
/// a description containing literal placeholder text can never be re-scanned
/// and spliced into the script as if it were data (#16). The serialized JSON
/// also has every `<` escaped to `\u003c` before insertion, so it cannot
/// contain `<!--`, `<script`, or `</script` and therefore cannot move the
/// HTML tokenizer out of script-data state.
pub(crate) const SEARCH_SCRIPT_TEMPLATE: &str = r#"    <script>
    (function () {
        const searchText = __SEARCH_INDEX__;
        const categoryIndex = __CATEGORY_INDEX__;
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

        // Fallback used inline if Workers aren't available; the same
        // logic also runs in the Worker below.
        function matchAll(query, category) {
            let regex = null;
            if (query !== '') {
                try {
                    regex = new RegExp(query, 'i');
                } catch (e) {
                    return { error: true };
                }
            }
            const visible = new Array(searchText.length);
            let count = 0;
            for (let i = 0; i < searchText.length; i++) {
                const matches = (!regex || regex.test(searchText[i])) && (!category || categoryIndex[i] === category);
                visible[i] = matches;
                if (matches) count++;
            }
            return { visible, count };
        }

        let worker = null;
        let requestId = 0;
        if (window.Worker) {
            try {
                const workerSource = `
                    let searchText = [];
                    let categoryIndex = [];
                    self.onmessage = function (e) {
                        const msg = e.data;
                        if (msg.type === 'init') {
                            searchText = msg.searchText;
                            categoryIndex = msg.categoryIndex;
                            return;
                        }
                        let regex = null;
                        if (msg.query !== '') {
                            try {
                                regex = new RegExp(msg.query, 'i');
                            } catch (err) {
                                self.postMessage({ id: msg.id, error: true });
                                return;
                            }
                        }
                        // Transferred via its buffer below - zero-copy, not structured-cloned.
                        const visible = new Uint8Array(searchText.length);
                        let count = 0;
                        for (let i = 0; i < searchText.length; i++) {
                            const matches = (!regex || regex.test(searchText[i])) && (!msg.category || categoryIndex[i] === msg.category);
                            visible[i] = matches ? 1 : 0;
                            if (matches) count++;
                        }
                        self.postMessage({ id: msg.id, visible, count }, [visible.buffer]);
                    };
                `;
                worker = new Worker(URL.createObjectURL(new Blob([workerSource], { type: 'application/javascript' })));
                worker.postMessage({ type: 'init', searchText, categoryIndex });
                worker.onmessage = (e) => {
                    if (e.data.id !== requestId) return; // superseded by a newer query
                    applyResult(e.data);
                };
                worker.onerror = () => {
                    worker = null;
                };
            } catch (e) {
                worker = null;
            }
        }

        // Chunked across frames so large documents don't drop one applying it.
        function applyVisibility(visible) {
            let i = 0;
            const chunkSize = 300;
            function step() {
                const end = Math.min(i + chunkSize, options.length);
                for (; i < end; i++) {
                    options[i].classList.toggle('search-hidden', !visible[i]);
                }
                if (i < options.length) {
                    requestAnimationFrame(step);
                }
            }
            requestAnimationFrame(step);
        }

        function applyResult(result) {
            if (result.error) {
                input.classList.add('invalid');
                status.classList.add('invalid');
                status.textContent = 'Invalid regular expression';
                return;
            }
            applyVisibility(result.visible);
            status.textContent = (input.value.trim() !== '' || activeCategory)
                ? `Showing ${result.count} of ${options.length} options`
                : '';
        }

        function runSearch() {
            const query = input.value.trim();
            input.classList.remove('invalid');
            status.classList.remove('invalid');

            requestId += 1;
            if (worker) {
                worker.postMessage({ id: requestId, query, category: activeCategory });
            } else {
                applyResult(matchAll(query, activeCategory));
            }
        }

        let searchDebounce = null;
        input.addEventListener('input', () => {
            clearTimeout(searchDebounce);
            searchDebounce = setTimeout(runSearch, 120);
        });

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

            --c-bool: #A66709;
            --c-choice: #697D1F;
            --c-string: #2563A8;
            --c-number: #1F7A4D;
            --c-package: #A63D8F;
            --c-list: #0E7C7B;
            --c-set: #7C3AED;
            --c-submodule: #B23A6B;
            --c-any: #5B6B78;

            --a-note: #1F6FEB;
            --a-tip: #2B9E4B;
            --a-important: #8250DF;
            --a-warning: #9A6700;
            --a-caution: #CF222E;

            /* 4px base at the root font-size; rem, not em, so nested
               font-sizes can't change what a step actually renders as. */
            --sp-1: 0.25rem;
            --sp-2: 0.5rem;
            --sp-3: 0.75rem;
            --sp-4: 1rem;
            --sp-5: 1.5rem;
            --sp-7: 3rem;
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

                --a-note: #1F6FEB;
                --a-tip: #2DA44E;
                --a-important: #8250DF;
                --a-warning: #9A6700;
                --a-caution: #D12B36;
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

            --a-note: #1F6FEB;
            --a-tip: #2DA44E;
            --a-important: #8250DF;
            --a-warning: #9A6700;
            --a-caution: #D12B36;
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

            --c-bool: #A66709;
            --c-choice: #697D1F;
            --c-string: #2563A8;
            --c-number: #1F7A4D;
            --c-package: #A63D8F;
            --c-list: #0E7C7B;
            --c-set: #7C3AED;
            --c-submodule: #B23A6B;
            --c-any: #5B6B78;

            --a-note: #1F6FEB;
            --a-tip: #2B9E4B;
            --a-important: #8250DF;
            --a-warning: #9A6700;
            --a-caution: #CF222E;
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
            max-width: 100%;
            overflow: auto;
        }
        pre code { background: transparent; padding: 0; font-size: 0.8em; }

        /* ---- masthead ---- */

        .masthead-top {
            display: flex;
            align-items: baseline;
            justify-content: space-between;
            gap: 1em;
        }
        .eyebrow {
            font-family: ui-monospace, "SF Mono", "Cascadia Code", monospace;
            font-size: 0.8em;
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

        .search-row { position: relative; margin-top: var(--sp-3); }
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
            font-size: 0.8em;
            color: var(--ink-muted);
            border: 1px solid var(--line);
            border-radius: 4px;
            padding: 0.05em 0.4em;
            pointer-events: none;
        }
        #search-status {
            margin-top: var(--sp-2);
            min-height: 1.2em;
            font-family: ui-monospace, "SF Mono", "Cascadia Code", monospace;
            font-size: 0.8em;
            color: var(--ink-muted);
        }
        #search-status.invalid { color: var(--danger); }

        .legend {
            display: flex;
            flex-wrap: wrap;
            gap: var(--sp-2);
            margin-top: var(--sp-3);
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
        .t-deprecated { --c: var(--danger); }
        .t-any { --c: var(--c-any); }

        /* ---- option entries (ledger style: hairlines, not boxes) ---- */

        .option {
            padding: 1.75em 0;
            border-bottom: 1px solid var(--line);
            /* Skips layout/paint for off-screen options on large documents. */
            content-visibility: auto;
            contain-intrinsic-size: 0 220px;
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
            font-size: 1.15em;
            font-weight: 600;
            margin: 0;
            word-break: break-word;
        }
        .option-path a { color: inherit; }
        .path-prefix { color: var(--ink-muted); font-weight: 400; }
        .path-leaf { color: var(--ink); }

        .option-desc {
            margin: var(--sp-3) 0 0;
            color: var(--ink);
        }
        .option-desc p:first-child { margin-top: 0; }
        .option-desc p:last-child { margin-bottom: 0; }

        .option-meta {
            display: flex;
            flex-wrap: wrap;
            gap: 0.5em 1.5em;
            margin: var(--sp-4) 0 0;
        }
        .meta-row { display: flex; align-items: baseline; gap: 0.5em; }
        .meta-row.block {
            flex-direction: column;
            align-items: stretch;
            flex-basis: 100%;
            gap: 0.3em;
            /* Without this, a flex item won't shrink below its content's
               intrinsic width - an unbroken long line (e.g. a URL) in the
               <pre> below would blow out the card instead of triggering
               its own overflow-x scrollbar. */
            min-width: 0;
        }
        .meta-label {
            font-family: ui-monospace, "SF Mono", "Cascadia Code", monospace;
            font-size: 0.72em;
            font-weight: 600;
            letter-spacing: 0.06em;
            text-transform: uppercase;
            color: var(--ink-muted);
        }

        .option-decl {
            margin-top: var(--sp-3);
            font-family: ui-monospace, "SF Mono", "Cascadia Code", monospace;
            font-size: 0.8em;
            color: var(--ink-muted);
        }
        .also-declared-label {
            /* Deliberately the scale's biggest gap - marks a real seam
               (other files vs. this option's own declaration above). */
            margin-top: var(--sp-5);
            font-family: ui-monospace, "SF Mono", "Cascadia Code", monospace;
            font-size: 0.72em;
            font-weight: 600;
            letter-spacing: 0.06em;
            text-transform: uppercase;
            color: var(--ink-muted);
        }
        .also-declared {
            margin-top: var(--sp-1);
            padding-left: 1em;
            list-style: none;
            font-family: ui-monospace, "SF Mono", "Cascadia Code", monospace;
            font-size: 0.8em;
            color: var(--ink-muted);
        }
        .also-declared li { margin-top: var(--sp-1); }
        .also-declared .alt-desc { color: var(--ink-muted); margin: var(--sp-1) 0 0; }

        /* ---- github-alert admonitions inside descriptions ---- */

        .markdown-alert {
            padding: var(--sp-3) var(--sp-4);
            margin: var(--sp-3) 0;
            border-radius: 6px;
            border-left: 3px solid var(--line);
            background: var(--surface-2);
        }
        .markdown-alert p { margin: var(--sp-2) 0; }
        .markdown-alert-title {
            font-weight: 700;
            font-size: 0.8em;
            letter-spacing: 0.03em;
            text-transform: uppercase;
            margin-bottom: 0.3em !important;
        }
        .markdown-alert-note { border-left-color: var(--a-note); }
        .markdown-alert-tip { border-left-color: var(--a-tip); }
        .markdown-alert-important { border-left-color: var(--a-important); }
        .markdown-alert-warning { border-left-color: var(--a-warning); }
        .markdown-alert-caution { border-left-color: var(--a-caution); }

        .footer {
            margin-top: var(--sp-7);
            padding-top: var(--sp-5);
            border-top: 1px solid var(--line);
            font-family: ui-monospace, "SF Mono", "Cascadia Code", monospace;
            font-size: 0.8em;
            color: var(--ink-muted);
        }

        @media (max-width: 520px) {
            .search-kbd { display: none; }
        }
    </style>
</head>
<body>
"#;
