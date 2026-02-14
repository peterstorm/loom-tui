use std::sync::LazyLock;

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

use crate::model::Theme;

const MAX_HIGHLIGHT_LINES: usize = 200;

static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static THEME_SET: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);

/// Highlight a code block with syntax coloring and line numbers.
pub fn highlight_code_block(lines: &[&str], extension: &str) -> Vec<Line<'static>> {
    let ss = &*SYNTAX_SET;
    let theme = best_theme();
    let syntax = ss
        .find_syntax_by_extension(extension)
        .unwrap_or_else(|| ss.find_syntax_plain_text());
    let mut h = HighlightLines::new(syntax, theme);

    let count = lines.len().min(MAX_HIGHLIGHT_LINES);
    let gutter_w = digit_width(count);

    lines[..count]
        .iter()
        .enumerate()
        .map(|(i, code)| {
            let mut spans = vec![gutter_span(i + 1, gutter_w)];
            spans.extend(highlight_line_spans(&mut h, code, ss));
            Line::from(spans)
        })
        .collect()
}

/// Highlight diff lines (+/-) with syntax coloring, prefix colors, and line numbers.
pub fn highlight_diff_block(lines: &[&str], extension: &str) -> Vec<Line<'static>> {
    let ss = &*SYNTAX_SET;
    let theme = best_theme();
    let syntax = ss
        .find_syntax_by_extension(extension)
        .unwrap_or_else(|| ss.find_syntax_plain_text());
    let mut h = HighlightLines::new(syntax, theme);

    let count = lines.len().min(MAX_HIGHLIGHT_LINES);
    let gutter_w = digit_width(count);

    lines[..count]
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let (prefix, code, prefix_color) = parse_diff_prefix(line);
            let is_removal = prefix == "- ";
            let mut spans = vec![
                gutter_span(i + 1, gutter_w),
                Span::styled(prefix.to_string(), Style::default().fg(prefix_color)),
            ];
            let mut code_spans = highlight_line_spans(&mut h, code, ss);
            if is_removal {
                for s in &mut code_spans {
                    s.style = s.style.add_modifier(Modifier::DIM);
                }
            }
            spans.extend(code_spans);
            Line::from(spans)
        })
        .collect()
}

/// Extract file extension from a path string or diff header.
/// Handles formats like:
/// - "src/foo.ts"
/// - "--- a/src/foo.ts"
/// - "+++ b/src/foo.ts"
/// - "diff --git a/src/foo.ts b/src/foo.ts"
pub fn detect_extension(line: &str) -> Option<String> {
    let path = line.trim();

    // Strip diff prefixes
    let clean_path = path
        .strip_prefix("--- a/").or_else(|| path.strip_prefix("--- "))
        .or_else(|| path.strip_prefix("+++ b/").or_else(|| path.strip_prefix("+++ ")))
        .or_else(|| {
            // Handle "diff --git a/file b/file"
            if path.starts_with("diff --git") {
                path.split_whitespace().nth(2)?.strip_prefix("a/")
            } else {
                None
            }
        })
        .unwrap_or(path);

    let filename = clean_path.rsplit('/').next().unwrap_or(clean_path);
    let ext = filename.rsplit('.').next()?;
    if ext == filename || ext.is_empty() {
        return None;
    }
    Some(ext.to_lowercase())
}

/// Map markdown language hint to file extension for syntect.
pub fn lang_to_extension(lang: &str) -> String {
    let lower = lang.trim().to_ascii_lowercase();
    match lower.as_str() {
        "rust" | "rs" => "rs",
        "javascript" | "js" => "js",
        // TypeScript not in default set - use JS highlighting
        "typescript" | "ts" | "tsx" => "js",
        "python" | "py" => "py",
        "java" => "java",
        "go" | "golang" => "go",
        "c" => "c",
        "cpp" | "c++" => "cpp",
        "ruby" | "rb" => "rb",
        "bash" | "sh" | "shell" | "zsh" => "sh",
        "json" => "json",
        "yaml" | "yml" => "yaml",
        "toml" => "toml",
        "html" => "html",
        "css" => "css",
        "sql" => "sql",
        "xml" => "xml",
        "markdown" | "md" => "md",
        other => return other.to_string(),
    }
    .to_string()
}

fn best_theme() -> &'static syntect::highlighting::Theme {
    let ts = &*THEME_SET;
    ts.themes
        .get("base16-eighties.dark")
        .or_else(|| ts.themes.get("base16-ocean.dark"))
        .or_else(|| ts.themes.values().next())
        .expect("no themes available")
}

fn gutter_span(line_num: usize, width: usize) -> Span<'static> {
    Span::styled(
        format!("{:>w$} │ ", line_num, w = width),
        Style::default()
            .fg(Theme::MUTED_TEXT)
            .add_modifier(Modifier::DIM),
    )
}

fn digit_width(count: usize) -> usize {
    count.max(1).to_string().len().max(2)
}

fn parse_diff_prefix(line: &str) -> (&str, &str, Color) {
    if let Some(code) = line.strip_prefix("+ ") {
        ("+ ", code, Theme::SUCCESS)
    } else if let Some(code) = line.strip_prefix("- ") {
        ("- ", code, Theme::ERROR)
    } else {
        ("  ", line, Theme::MUTED_TEXT)
    }
}

fn highlight_line_spans(
    h: &mut HighlightLines,
    line: &str,
    ss: &SyntaxSet,
) -> Vec<Span<'static>> {
    // SyntaxSet::load_defaults_newlines requires lines to end with \n
    // for syntax regexes to match correctly.
    let line_nl = format!("{}\n", line);
    h.highlight_line(&line_nl, ss)
        .unwrap_or_default()
        .into_iter()
        .map(|(style, text)| {
            let fg = style.foreground;
            Span::styled(
                text.trim_end_matches('\n').to_string(),
                Style::default().fg(Color::Rgb(fg.r, fg.g, fg.b)),
            )
        })
        .filter(|span| !span.content.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_extension_rs() {
        assert_eq!(detect_extension("src/main.rs"), Some("rs".into()));
    }

    #[test]
    fn detect_extension_nested_path() {
        assert_eq!(
            detect_extension("src/view/components/syntax.rs"),
            Some("rs".into())
        );
    }

    #[test]
    fn detect_extension_no_ext() {
        assert_eq!(detect_extension("Makefile"), None);
    }

    #[test]
    fn detect_extension_trims() {
        assert_eq!(detect_extension("  foo.py  "), Some("py".into()));
    }

    #[test]
    fn lang_to_extension_maps_known() {
        assert_eq!(lang_to_extension("rust"), "rs");
        assert_eq!(lang_to_extension("javascript"), "js");
        assert_eq!(lang_to_extension("Python"), "py");
    }

    #[test]
    fn lang_to_extension_passthrough_unknown() {
        assert_eq!(lang_to_extension("haskell"), "haskell");
    }

    #[test]
    fn highlight_code_block_returns_correct_line_count() {
        let code = vec!["fn main() {}", "    println!(\"hi\");", "}"];
        let lines = highlight_code_block(&code, "rs");
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn highlight_code_block_has_gutter() {
        let code = vec!["let x = 1;"];
        let lines = highlight_code_block(&code, "rs");
        let first_span = &lines[0].spans[0];
        assert!(first_span.content.contains("1"));
        assert!(first_span.content.contains("│"));
    }

    #[test]
    fn highlight_code_block_caps_at_max() {
        let code: Vec<&str> = (0..300).map(|_| "let x = 1;").collect();
        let lines = highlight_code_block(&code, "rs");
        assert_eq!(lines.len(), MAX_HIGHLIGHT_LINES);
    }

    #[test]
    fn highlight_diff_block_preserves_prefix_colors() {
        let diff = vec!["+ added line", "- removed line"];
        let lines = highlight_diff_block(&diff, "rs");
        assert_eq!(lines.len(), 2);
        // spans[0] = gutter, spans[1] = prefix
        assert_eq!(lines[0].spans[1].style.fg, Some(Theme::SUCCESS));
        assert_eq!(lines[1].spans[1].style.fg, Some(Theme::ERROR));
    }

    #[test]
    fn highlight_diff_block_has_gutter() {
        let diff = vec!["+ let x = 1;"];
        let lines = highlight_diff_block(&diff, "rs");
        let gutter = &lines[0].spans[0];
        assert!(gutter.content.contains("1"));
        assert!(gutter.content.contains("│"));
    }

    #[test]
    fn highlight_produces_non_white_colors() {
        let code = vec!["fn main() {}", "    let x = 42;"];
        let lines = highlight_code_block(&code, "rs");
        // Code spans start after gutter (index 0)
        let has_color = lines.iter().any(|line| {
            line.spans.iter().skip(1).any(|span| {
                matches!(span.style.fg, Some(Color::Rgb(r, g, b)) if !(r == 255 && g == 255 && b == 255))
            })
        });
        assert!(has_color, "syntax highlighting should produce non-white colors");
    }

    #[test]
    fn typescript_uses_javascript_highlighting() {
        assert_eq!(lang_to_extension("typescript"), "js");
        assert_eq!(lang_to_extension("ts"), "js");
        assert_eq!(lang_to_extension("tsx"), "js");
    }

    #[test]
    fn sql_highlighting_works() {
        let code = vec!["SELECT * FROM users", "WHERE id = 1;"];
        let lines = highlight_code_block(&code, "sql");
        assert_eq!(lines.len(), 2);
        // Verify we got highlighting (not just plain text)
        assert!(lines[0].spans.len() > 1, "SQL should be syntax highlighted");
    }

    #[test]
    fn detect_extension_handles_diff_headers() {
        assert_eq!(detect_extension("--- a/src/foo.ts"), Some("ts".into()));
        assert_eq!(detect_extension("+++ b/src/bar.rs"), Some("rs".into()));
        assert_eq!(detect_extension("--- src/test.py"), Some("py".into()));
        assert_eq!(detect_extension("+++ lib/main.js"), Some("js".into()));
    }

    #[test]
    fn detect_extension_handles_git_diff() {
        assert_eq!(
            detect_extension("diff --git a/src/app.ts b/src/app.ts"),
            Some("ts".into())
        );
    }

    #[test]
    fn detect_extension_plain_path() {
        assert_eq!(detect_extension("src/components/Button.tsx"), Some("tsx".into()));
        assert_eq!(detect_extension("/absolute/path/file.sql"), Some("sql".into()));
    }
}
