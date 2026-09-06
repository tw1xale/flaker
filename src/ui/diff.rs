use crate::theme::{self, Theme};
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

/// Parses the target path from a `diff --git a/path b/path` line.
fn parse_diff_git_path(line: &str) -> String {
    if let Some(rest) = line.strip_prefix("diff --git ") {
        if let Some((_a, b)) = rest.split_once(" b/") {
            return b.to_string();
        } else if let Some((a, _b)) = rest.split_once(" a/") {
            return a.to_string();
        } else {
            return rest.to_string();
        }
    }
    line.to_string()
}

/// Parses coordinates and optional section heading from a hunk header `@@ -old +new @@ [heading]`.
fn parse_hunk_header(line: &str) -> (usize, String) {
    let Some(rest) = line.strip_prefix("@@") else {
        return (0, String::new());
    };
    let Some((coords, heading)) = rest.split_once("@@") else {
        return (0, String::new());
    };

    let heading = heading.trim().to_string();
    let mut line_num = 0;
    for part in coords.split_whitespace() {
        if let Some(plus_part) = part.strip_prefix('+') {
            let num_str = plus_part.split(',').next().unwrap_or("0");
            if let Ok(n) = num_str.parse::<usize>() {
                line_num = n;
                break;
            }
        }
    }
    (line_num, heading)
}

/// Expands tab characters into spaces based on standard tab stops and strips harmful control characters.
///
/// Ensures terminal cursor alignment remains in sync with Ratatui's buffer coordinate system,
/// preventing text overflow past widget boundaries and ghost artifacts from skipped cells.
pub fn expand_tabs(line: &str, tab_width: usize) -> String {
    if !line.contains('\t')
        && !line.contains('\r')
        && !line.chars().any(|c| (c as u32) < 32 && c != '\n')
    {
        return line.to_string();
    }

    let tab_width = tab_width.max(1);
    let mut result = String::with_capacity(line.len() + 16);
    let mut col = 0;

    for ch in line.chars() {
        match ch {
            '\t' => {
                let spaces = tab_width - (col % tab_width);
                for _ in 0..spaces {
                    result.push(' ');
                }
                col += spaces;
            }
            '\n' => {
                result.push('\n');
                col = 0;
            }
            '\r' => {
                // Strip carriage returns
            }
            c if (c as u32) < 32 => {
                // Sanitize non-printable control characters that could disrupt terminal cursor
                result.push(' ');
                col += 1;
            }
            c => {
                result.push(c);
                col += 1;
            }
        }
    }

    result
}

/// Converts raw git show/diff output into structured, clean UI lines.
///
/// Filters out low-level Git plumbing lines (`diff --git`, `index`, `---`, `+++`)
/// and formats commit metadata, changed files, and hunk offsets into human-readable markers.
pub fn prettify_diff(raw: &str) -> Vec<String> {
    if raw.trim().is_empty() {
        return Vec::new();
    }

    // Expand tabs upfront to ensure correct indentation and prevent terminal cursor desync
    let expanded = expand_tabs(raw, 4);
    let mut result = Vec::new();
    let mut lines = expanded.lines().peekable();

    // 1. Commit metadata parsing (if starting with "commit <hash>")
    if lines.peek().is_some_and(|l| l.starts_with("commit ")) {
        let commit_line = lines.next().unwrap();
        let hash = commit_line.strip_prefix("commit ").unwrap_or("").trim();
        let short_hash = if hash.len() > 8 { &hash[..8] } else { hash };
        result.push(format!("§§meta:Commit:{short_hash}"));

        while let Some(&line) = lines.peek() {
            if line.starts_with("Author:") {
                let author = line.strip_prefix("Author:").unwrap_or("").trim();
                result.push(format!("§§meta:Author:{author}"));
                lines.next();
            } else if line.starts_with("Date:") {
                let date = line.strip_prefix("Date:").unwrap_or("").trim();
                result.push(format!("§§meta:Date:{date}"));
                lines.next();
            } else if line.starts_with("Merge:") {
                let merge = line.strip_prefix("Merge:").unwrap_or("").trim();
                result.push(format!("§§meta:Merge:{merge}"));
                lines.next();
            } else if line.trim().is_empty() {
                lines.next();
                break;
            } else {
                break;
            }
        }

        // Commit message
        let mut message_lines = Vec::new();
        while let Some(&line) = lines.peek() {
            if line == "---" || line.starts_with("diff --git ") {
                break;
            }
            let trimmed = line.strip_prefix("    ").unwrap_or(line).trim_end();
            message_lines.push(trimmed.to_string());
            lines.next();
        }

        while message_lines.first().is_some_and(|s| s.is_empty()) {
            message_lines.remove(0);
        }
        while message_lines.last().is_some_and(|s| s.is_empty()) {
            message_lines.pop();
        }

        for (idx, msg) in message_lines.into_iter().enumerate() {
            if idx == 0 {
                result.push(format!("§§meta:Message:{msg}"));
            } else {
                result.push(format!("§§meta:MessageCont:{msg}"));
            }
        }

        // Stat summary
        if lines.peek().is_some_and(|&l| l == "---") {
            lines.next();
            let mut stat_lines = Vec::new();
            while let Some(&stat_candidate) = lines.peek() {
                if stat_candidate.starts_with("diff --git ") {
                    break;
                }
                let trimmed = stat_candidate.trim_end();
                if !trimmed.is_empty() {
                    stat_lines.push(trimmed.to_string());
                }
                lines.next();
            }
            if !stat_lines.is_empty() {
                result.push("§§divider:Summary".to_string());
                result.extend(stat_lines);
            }
        }
    }

    // 2. Diff parsing (files, hunks, additions, deletions)
    let mut current_file: Option<String> = None;
    let mut file_status = String::new();
    let mut file_header_emitted = false;

    for line in lines {
        if line.starts_with("diff --git ") {
            if !file_header_emitted && let Some(prev_file) = current_file.take() {
                result.push(format!("§§file:{prev_file}{file_status}"));
                result.push("  (no content changes)".to_string());
            }
            current_file = Some(parse_diff_git_path(line));
            file_status.clear();
            file_header_emitted = false;
        } else if line.starts_with("new file mode") {
            file_status = " (new file)".to_string();
        } else if line.starts_with("deleted file mode") {
            file_status = " (deleted)".to_string();
        } else if line.starts_with("similarity index") {
            file_status = " (renamed)".to_string();
        } else if line.starts_with("index ")
            || line.starts_with("--- ")
            || line.starts_with("+++ ")
            || line.starts_with("rename from")
            || line.starts_with("rename to")
        {
            // Skip raw Git plumbing lines
            continue;
        } else if line.starts_with("@@") {
            if !file_header_emitted && let Some(ref file) = current_file {
                result.push(format!("§§file:{file}{file_status}"));
                file_header_emitted = true;
            }
            let (line_num, heading) = parse_hunk_header(line);
            result.push(format!("§§hunk:{line_num}:{heading}"));
        } else if line.starts_with('+') || line.starts_with('-') || line.starts_with(' ') {
            if !file_header_emitted && let Some(ref file) = current_file {
                result.push(format!("§§file:{file}{file_status}"));
                file_header_emitted = true;
            }
            result.push(line.to_string());
        } else if line.starts_with(r"\ No newline at end of file") {
            result.push(r"§§hint:\ No newline at end of file".to_string());
        } else {
            let trimmed = line.trim_end();
            if !trimmed.is_empty() {
                result.push(trimmed.to_string());
            }
        }
    }

    if !file_header_emitted && let Some(prev_file) = current_file {
        result.push(format!("§§file:{prev_file}{file_status}"));
        result.push("  (no content changes)".to_string());
    }

    result
}

/// Styles an individual line from prettified (or raw) diff into a Ratatui `Line`.
pub fn format_diff_line(raw: &str, theme: &Theme) -> Line<'static> {
    let clean = expand_tabs(raw, 4);
    let s = clean.as_str();

    if let Some(hash) = s.strip_prefix("§§meta:Commit:") {
        Line::from(vec![
            Span::styled(" Commit:  ", Style::default().fg(theme.secondary_info)),
            Span::styled(
                hash.to_string(),
                Style::default()
                    .fg(theme.selected)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    } else if let Some(author) = s.strip_prefix("§§meta:Author:") {
        Line::from(vec![
            Span::styled(" Author:  ", Style::default().fg(theme.faint_hint)),
            Span::styled(author.to_string(), Style::default().fg(theme.text)),
        ])
    } else if let Some(date) = s.strip_prefix("§§meta:Date:") {
        Line::from(vec![
            Span::styled(" Date:    ", Style::default().fg(theme.faint_hint)),
            Span::styled(date.to_string(), Style::default().fg(theme.neutral_text)),
        ])
    } else if let Some(merge) = s.strip_prefix("§§meta:Merge:") {
        Line::from(vec![
            Span::styled(" Merge:   ", Style::default().fg(theme.faint_hint)),
            Span::styled(merge.to_string(), Style::default().fg(theme.accent)),
        ])
    } else if let Some(msg) = s.strip_prefix("§§meta:Message:") {
        Line::from(vec![
            Span::styled(" Message: ", Style::default().fg(theme.faint_hint)),
            Span::styled(
                msg.to_string(),
                Style::default()
                    .fg(theme.header_title)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    } else if let Some(msg) = s.strip_prefix("§§meta:MessageCont:") {
        Line::from(vec![
            Span::raw("          "),
            Span::styled(msg.to_string(), Style::default().fg(theme.text)),
        ])
    } else if let Some(label) = s.strip_prefix("§§divider:") {
        let rule_len = 50usize.saturating_sub(label.chars().count() + 4).max(10);
        let rule = "─".repeat(rule_len);
        Line::from(vec![
            Span::styled("── ", Style::default().fg(theme.border)),
            Span::styled(
                label.to_string(),
                Style::default()
                    .fg(theme.secondary_info)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!(" {rule}"), Style::default().fg(theme.border)),
        ])
    } else if let Some(file_info) = s.strip_prefix("§§file:") {
        let rule = "─".repeat(45);
        Line::from(vec![
            Span::styled("── ", Style::default().fg(theme.border)),
            Span::styled(
                format!("{} ", theme::ICON_FILE),
                Style::default().fg(theme.accent),
            ),
            Span::styled(
                file_info.to_string(),
                Style::default()
                    .fg(theme.header_title)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!(" {rule}"), Style::default().fg(theme.border)),
        ])
    } else if let Some(rest) = s.strip_prefix("§§hunk:") {
        let (line_num_str, heading) = rest.split_once(':').unwrap_or((rest, ""));
        let line_title = if line_num_str == "0" {
            "Changes".to_string()
        } else if heading.is_empty() {
            format!("Line {line_num_str}")
        } else {
            format!("Line {line_num_str}: {heading}")
        };
        let rule = "┄".repeat(45);
        Line::from(vec![
            Span::styled("   ┄ ", Style::default().fg(theme.faint_hint)),
            Span::styled(
                line_title,
                Style::default()
                    .fg(theme.secondary_info)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!(" {rule}"), Style::default().fg(theme.faint_hint)),
        ])
    } else if let Some(hint) = s.strip_prefix("§§hint:") {
        Line::from(Span::styled(
            hint.to_string(),
            Style::default()
                .fg(theme.faint_hint)
                .add_modifier(Modifier::ITALIC),
        ))
    } else if s.starts_with('+') && !s.starts_with("+++") {
        Line::from(vec![
            Span::styled(
                "+",
                Style::default()
                    .fg(theme.success)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(s[1..].to_string(), Style::default().fg(theme.success)),
        ])
    } else if s.starts_with('-') && !s.starts_with("---") {
        Line::from(vec![
            Span::styled(
                "-",
                Style::default()
                    .fg(theme.danger)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(s[1..].to_string(), Style::default().fg(theme.danger)),
        ])
    } else if s.contains('|') && (s.contains('+') || s.contains('-')) {
        let parts: Vec<&str> = s.splitn(2, '|').collect();
        if parts.len() == 2 {
            let mut spans = vec![
                Span::styled(parts[0].to_string(), Style::default().fg(theme.text)),
                Span::styled("|", Style::default().fg(theme.faint_hint)),
            ];
            for ch in parts[1].chars() {
                let style = match ch {
                    '+' => Style::default()
                        .fg(theme.success)
                        .add_modifier(Modifier::BOLD),
                    '-' => Style::default()
                        .fg(theme.danger)
                        .add_modifier(Modifier::BOLD),
                    _ => Style::default().fg(theme.neutral_text),
                };
                spans.push(Span::styled(ch.to_string(), style));
            }
            Line::from(spans)
        } else {
            Line::from(Span::styled(s.to_string(), Style::default().fg(theme.text)))
        }
    } else if s.starts_with("@@") || s.contains("file changed") || s.contains("files changed") {
        Line::from(Span::styled(
            s.to_string(),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ))
    } else if s.starts_with("diff --git") || s.starts_with("index ") {
        Line::from(Span::styled(
            s.to_string(),
            Style::default()
                .fg(theme.secondary_info)
                .add_modifier(Modifier::BOLD),
        ))
    } else if s.starts_with("commit ") {
        Line::from(Span::styled(
            s.to_string(),
            Style::default()
                .fg(theme.selected)
                .add_modifier(Modifier::BOLD),
        ))
    } else if s.starts_with("Author:") || s.starts_with("Date:") {
        Line::from(Span::styled(s.to_string(), Style::default().fg(theme.neutral_text)))
    } else if s.contains("(current)") {
        Line::from(Span::styled(
            s.to_string(),
            Style::default()
                .fg(theme.success)
                .add_modifier(Modifier::BOLD),
        ))
    } else {
        Line::from(Span::styled(s.to_string(), Style::default().fg(theme.text)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_diff_git_path() {
        assert_eq!(
            parse_diff_git_path("diff --git a/flake.lock b/flake.lock"),
            "flake.lock"
        );
        assert_eq!(
            parse_diff_git_path("diff --git a/src/app.rs b/src/app.rs"),
            "src/app.rs"
        );
    }

    #[test]
    fn test_parse_hunk_header() {
        let (num, heading) = parse_hunk_header("@@ -60,11 +60,11 @@");
        assert_eq!(num, 60);
        assert_eq!(heading, "");

        let (num, heading) =
            parse_hunk_header("@@ -132,6 +132,7 @@ All system administration actions:");
        assert_eq!(num, 132);
        assert_eq!(heading, "All system administration actions:");

        let (num, heading) = parse_hunk_header("@@ -1 +1 @@");
        assert_eq!(num, 1);
        assert_eq!(heading, "");
    }

    #[test]
    fn test_prettify_diff_git_show() {
        let raw = r#"commit 01dc6c7b5e6ed37af0606a23b9a1af831715fa76
Author: tw1xale <scyrox@icloud.com>
Date:   Sun Sep 6 12:40:37 2026 +0500

    full update
---
 flake.lock | 6 +++---
 1 file changed, 3 insertions(+), 3 deletions(-)

diff --git a/flake.lock b/flake.lock
index 7b691df..f75a537 100644
--- a/flake.lock
+++ b/flake.lock
@@ -60,11 +60,11 @@
             ]
         },
         "locked": {
-            "lastModified": 1788372954,
+            "lastModified": 1788412837,
"#;

        let prettified = prettify_diff(raw);
        assert!(prettified.iter().any(|l| l == "§§meta:Commit:01dc6c7b"));
        assert!(
            prettified
                .iter()
                .any(|l| l == "§§meta:Author:tw1xale <scyrox@icloud.com>")
        );
        assert!(prettified.iter().any(|l| l == "§§meta:Message:full update"));
        assert!(prettified.iter().any(|l| l == "§§divider:Summary"));
        assert!(
            prettified
                .iter()
                .any(|l| l.contains("flake.lock | 6 +++---"))
        );
        assert!(prettified.iter().any(|l| l == "§§file:flake.lock"));
        assert!(prettified.iter().any(|l| l == "§§hunk:60:"));
        assert!(
            prettified
                .iter()
                .any(|l| l == "-            \"lastModified\": 1788372954,")
        );
        assert!(
            prettified
                .iter()
                .any(|l| l == "+            \"lastModified\": 1788412837,")
        );
        // Ensure raw plumbing is stripped
        assert!(!prettified.iter().any(|l| l.starts_with("diff --git")));
        assert!(!prettified.iter().any(|l| l.starts_with("index ")));
        assert!(!prettified.iter().any(|l| l.starts_with("--- a/")));
        assert!(!prettified.iter().any(|l| l.starts_with("+++ b/")));
    }

    #[test]
    fn test_prettify_diff_single_file() {
        let raw = r#"diff --git a/src/theme.rs b/src/theme.rs
index 4a5dcb9..b68291a 100644
--- a/src/theme.rs
+++ b/src/theme.rs
@@ -299,6 +299,8 @@ pub const ICON_ERROR: &str = "\u{f057}";
  context
-old
+new
"#;
        let prettified = prettify_diff(raw);
        assert!(prettified.iter().any(|l| l == "§§file:src/theme.rs"));
        assert!(prettified.iter().any(|l| l.starts_with("§§hunk:299:")));
        assert!(prettified.iter().any(|l| l == "+new"));
        assert!(prettified.iter().any(|l| l == "-old"));
    }

    #[test]
    fn test_prettify_diff_new_file() {
        let raw = r#"diff --git a/new.txt b/new.txt
new file mode 100644
index 0000000..1234567
--- /dev/null
+++ b/new.txt
@@ -0,0 +1,2 @@
+line 1
+line 2
"#;
        let prettified = prettify_diff(raw);
        assert!(prettified.iter().any(|l| l == "§§file:new.txt (new file)"));
        assert!(prettified.iter().any(|l| l == "§§hunk:1:"));
    }

    #[test]
    fn test_expand_tabs() {
        assert_eq!(expand_tabs("hello", 4), "hello");
        assert_eq!(expand_tabs("\thello", 4), "    hello");
        assert_eq!(expand_tabs("+\thello", 4), "+   hello");
        assert_eq!(expand_tabs("-\thello", 4), "-   hello");
        assert_eq!(expand_tabs(" \thello", 4), "    hello");
        assert_eq!(expand_tabs("+\t\thello", 4), "+       hello");
        assert_eq!(expand_tabs("line\r\n", 4), "line\n");
        assert_eq!(expand_tabs("line1\n+\thello\n", 4), "line1\n+   hello\n");
    }

    #[test]
    fn test_prettify_diff_with_tabs() {
        let raw = "diff --git a/file.lua b/file.lua\n@@ -1,2 +1,2 @@\n-\told_code()\n+\tnew_code()\n";
        let prettified = prettify_diff(raw);
        assert!(prettified.iter().any(|l| l == "-   old_code()"));
        assert!(prettified.iter().any(|l| l == "+   new_code()"));
    }

    #[test]
    fn test_format_diff_line_with_tabs() {
        let theme = Theme::default();
        let line = format_diff_line("+\thello_world()", &theme);
        // Ensure no tab remains in any span
        for span in &line.spans {
            assert!(!span.content.contains('\t'));
        }
    }

    #[test]
    fn test_render_diff_line_into_buffer_no_spill() {
        use ratatui::buffer::Buffer;
        use ratatui::layout::Rect;
        use ratatui::widgets::{Paragraph, Widget};

        let theme = Theme::default();
        let raw = "+\thl.exec_cmd(\"dbus-update-activation-environment\")";
        let line = format_diff_line(raw, &theme);

        let mut buf = Buffer::empty(Rect::new(0, 0, 30, 1));
        let p = Paragraph::new(line);
        // Render into a constrained area of width 15
        p.render(Rect::new(0, 0, 15, 1), &mut buf);

        // Verify alignment
        assert_eq!(buf[(0, 0)].symbol(), "+");
        assert_eq!(buf[(1, 0)].symbol(), " ");
        assert_eq!(buf[(2, 0)].symbol(), " ");
        assert_eq!(buf[(3, 0)].symbol(), " ");
        assert_eq!(buf[(4, 0)].symbol(), "h");
        assert_eq!(buf[(5, 0)].symbol(), "l");

        // Verify strictly no characters written outside rendered rect (x >= 15)
        for x in 15..30 {
            assert_eq!(
                buf[(x, 0)].symbol(),
                " ",
                "Cell at x={x} should be untouched"
            );
        }
    }
}

