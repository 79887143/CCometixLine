# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

CCometixLine (`ccline`) is a Rust CLI statusline tool for Claude Code. Claude Code pipes a JSON payload to its stdin on each render; `ccline` reads it, collects per-segment data, and prints a single ANSI-colored statusline. It also ships a ratatui TUI configurator, a theme system, a Claude Code patcher, and an npm-registry update checker.

The Cargo binary name is `ccometixline`; when installed it is renamed to `ccline` (e.g. copied to `~/.claude/ccline/ccline`).

## Build, test, lint

```bash
cargo build                         # dev build
cargo build --release               # optimized build (used for releases)
cargo test                          # note: the repo currently has no unit tests; tests/ is gitignored
cargo fmt --all -- --check          # format check (pre-commit)
cargo clippy -- -D warnings         # lints, warnings are errors (pre-commit)
```

`lefthook.yml` runs `fmt-check` and `clippy` in parallel on pre-commit. Code must pass both `cargo fmt` and `cargo clippy -D warnings` before committing.

### Running locally

The entrypoint (`src/main.rs`) branches on whether stdin is a terminal:

- **Interactive** (stdin is a TTY): launches the `MainMenu` TUI — this is what users see when running `ccline` with no input.
- **Piped** (stdin has data): parses stdin as the Claude Code `InputData` JSON and renders the statusline.

So to exercise the statusline renderer manually, pipe JSON:

```bash
echo '{"model":{"id":"claude-sonnet-4","display_name":"Sonnet 4"},"workspace":{"current_dir":"'"$PWD"'"},"transcript_path":"/dev/null","cost":null,"output_style":null}' | cargo run --release
```

CLI flags (`src/cli.rs`): `-c/--config` (open configurator), `-t/--theme <name>` (temporary theme override), `--patch <cli.js>` (patch Claude Code).

## Architecture

### Entrypoint dispatch (`src/main.rs`)
Parses CLI → handles `--config` (TUI configurator) and `--patch` (patcher) → loads `Config` → applies `--theme` override → checks `stdin().is_terminal()` → either runs `MainMenu` or reads `InputData` from stdin, collects segments, renders, prints.

### Config (`src/config/`)
- **`types.rs`** — the core `Config` struct: `{ style: StyleConfig, segments: Vec<SegmentConfig>, theme: String }`. `SegmentConfig` carries `id`, `enabled`, `icon {plain, nerd_font}`, `colors {icon, text, background}` (each an `Option<AnsiColor>`), `styles {text_bold}`, and an `options: HashMap<String, serde_json::Value>` for segment-specific knobs. `AnsiColor` is an `untagged` enum: `Color16 { c16 }`, `Color256 { c256 }`, `Rgb { r, g, b }`. `InputData` is the stdin payload schema.
- **`loader.rs`** — `Config::load()` reads `~/.claude/ccline/config.toml`. **Two side effects on every load:** (1) `ensure_themes_exist()` *always overwrites* built-in theme `.toml` files on disk so files stay in sync with code; (2) `migrate_missing_segments()` appends any segments present in the current theme preset but missing from the user's config (this is how newly added segments reach existing installs). When adding a new `SegmentId`, it must be registered here, in `collect_all_segments`, and in the theme presets, or it won't appear.
- **`models.rs`** — `ModelConfig` (`~/.claude/ccline/models.toml`) resolves a model ID → display name + context limit via layered matching, in priority order: **context modifiers** (e.g. `[1m]` → 1M context + " 1M" suffix) > **user `[[models]]` entries** (simple substring match) > **built-in Claude families** (regex with auto version extraction for `sonnet`/`opus`/`haiku`, e.g. `claude-opus-4-6-…` → "Opus 4.6") > **default 200_000**. Regexes are compiled once via `OnceLock`.
- **`defaults.rs`** — `Config::default()` delegates to `ThemePresets::get_default()`; the theme presets are the single source of truth for defaults.

### Segments (`src/core/`)
- **`segments/mod.rs`** — the `Segment` trait: `fn collect(&self, input: &InputData) -> Option<SegmentData>` (+ `id()`). `SegmentData { primary, secondary, metadata: HashMap<String,String> }`. `metadata["dynamic_icon"]`, if set, overrides the configured icon at render time.
- **`statusline.rs`** — `collect_all_segments()` maps each `SegmentConfig.id` → the right segment constructor, reading `options` for segments that need them (`Git` → `show_sha`; `GlmCodingPlan` → `api_url`/`token`/`cache_duration`). `StatusLineGenerator` renders each enabled segment to an ANSI string and joins them. **Powerline mode** is triggered by `style.separator == "\u{e0b0}"` and renders arrow separators with foreground/background color transitions between adjacent segment backgrounds. Separate `generate_for_tui` / `generate_for_tui_preview` methods produce ratatui `Line`/`Text` with per-segment width-aware wrapping.
- Adding a segment: add the `SegmentId` variant, the `src/core/segments/<name>.rs` module implementing `Segment`, the match arm in `collect_all_segments`, the segment in every theme preset under `src/ui/themes/theme_*.rs`, and any UI match arm in `src/ui/` (e.g. `segment_list`). The commit `8f27152` (GlmCodingPlan added to all SegmentId match arms) is the reference example of the full set of touch points.

### Token usage & context
`RawUsage` (in `types.rs`) ingests provider usage with separate fields for Anthropic- and OpenAI-style naming and normalizes to `NormalizedUsage`, merging with Anthropic priority. `context_tokens()` (input + cache creation + cache read + output) drives the ContextWindow percentage. `ContextWindowSegment` parses the transcript `.jsonl` at `input.transcript_path` to sum context tokens; the limit comes from `ModelConfig::get_context_limit()`. `UsageSegment` and `GlmCodingPlanSegment` make HTTP calls (ureq) and cache results to files in `~/.claude/ccline/`.

### TUI (`src/ui/`)
ratatui + crossterm. `app.rs` (`App`) is the full configurator (`--config`); `main_menu.rs` is the interactive launcher shown with no stdin. `components/` are individual editor widgets (color picker, icon selector, theme selector, segment list, etc.). `events.rs` maps keys to `AppEvent`s.

### Themes (`src/ui/themes/`)
Each theme (`theme_*.rs`) is a function returning a full `Config` (style + ordered segments with colors/icons). `ThemePresets::get_theme(name)` loads `~/.claude/ccline/themes/<name>.toml` if present, else falls back to the built-in function. Because themes *are* configs, switching themes replaces the entire segment list — `Config::is_modified_from_theme()` / `matches_theme()` detect user edits on top of a preset.

### Utilities (`src/utils/`)
- **`claude_code_patcher.rs`** — `--patch` parses Claude Code's bundled `cli.js` with **tree-sitter** (`tree-sitter-javascript`) to locate and rewrite the context-low-warning and verbose-flag logic; creates `<cli.js>.backup` first and is version-aware via the `// Version:` header.
- **`credentials.rs`** — reads the Claude OAuth token for usage API calls. On macOS uses the Keychain (`security find-generic-password`); elsewhere reads `.credentials.json`, honoring `CLAUDE_CONFIG_DIR` if set.

### Updater (`src/updater.rs`)
Checks the npm registry (`@cometix/ccline`) for a newer version (not GitHub Releases, to avoid rate limits) and persists state to `~/.claude/ccline/.update_state.json`.

## Runtime config locations

- Config: `~/.claude/ccline/config.toml`
- Themes: `~/.claude/ccline/themes/*.toml` (built-ins always rewritten on load)
- Models: `~/.claude/ccline/models.toml` (auto-created on first run)
- OAuth token: macOS Keychain, or `$CLAUDE_CONFIG_DIR/.credentials.json` / `~/.claude/.credentials.json`

## Commit & release conventions

Conventional Commits are **required** — `git-cliff` (`cliff.toml`) runs with `filter_unconventional = true`. Types map to changelog groups: `feat`→Added, `fix`→Fixed, `perf`→Performance, `refactor`/`improve`→Changed, `style`→Styling, `docs`→Documentation, `test`→Testing, `chore`→Miscellaneous. Use a scope matching the subsystem (e.g. `feat(glm):`, `fix(config):`, `feat(segments):`, `refactor(patcher):`). Release commits use `chore: release vX.Y.Z` (skipped in the changelog). Bump the version in `Cargo.toml`; the GitHub workflow builds, creates a GitHub Release (git-cliff notes), and publishes per-platform npm packages driven by `npm/scripts/prepare-packages.js`.
