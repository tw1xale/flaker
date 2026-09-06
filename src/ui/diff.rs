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

/// Converts raw git show/diff output into structured, clean UI lines.
///
/// Filters out low-level Git plumbing lines (`diff --git`, `index`, `---`, `+++`)
/// and formats commit metadata, changed files, and hunk offsets into human-readable markers.
pub fn prettify_diff(raw: &str) -> Vec<String> {
    if raw.trim().is_empty() {
        return Vec::new();
    }

    let mut result = Vec::new();
    let mut lines = raw.lines().peekable();

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
pub fn format_diff_line<'a>(raw: &'a str, theme: &'a Theme) -> Line<'a> {
    if let Some(hash) = raw.strip_prefix("§§meta:Commit:") {
        Line::from(vec![
            Span::styled(" Commit:  ", Style::default().fg(theme.secondary_info)),
            Span::styled(
                hash,
                Style::default()
                    .fg(theme.selected)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    } else if let Some(author) = raw.strip_prefix("§§meta:Author:") {
        Line::from(vec![
            Span::styled(" Author:  ", Style::default().fg(theme.faint_hint)),
            Span::styled(author, Style::default().fg(theme.text)),
        ])
    } else if let Some(date) = raw.strip_prefix("§§meta:Date:") {
        Line::from(vec![
            Span::styled(" Date:    ", Style::default().fg(theme.faint_hint)),
            Span::styled(date, Style::default().fg(theme.neutral_text)),
        ])
    } else if let Some(merge) = raw.strip_prefix("§§meta:Merge:") {
        Line::from(vec![
            Span::styled(" Merge:   ", Style::default().fg(theme.faint_hint)),
            Span::styled(merge, Style::default().fg(theme.accent)),
        ])
    } else if let Some(msg) = raw.strip_prefix("§§meta:Message:") {
        Line::from(vec![
            Span::styled(" Message: ", Style::default().fg(theme.faint_hint)),
            Span::styled(
                msg,
                Style::default()
                    .fg(theme.header_title)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    } else if let Some(msg) = raw.strip_prefix("§§meta:MessageCont:") {
        Line::from(vec![
            Span::raw("          "),
            Span::styled(msg, Style::default().fg(theme.text)),
        ])
    } else if let Some(label) = raw.strip_prefix("§§divider:") {
        let rule_len = 50usize.saturating_sub(label.chars().count() + 4).max(10);
        let rule = "─".repeat(rule_len);
        Line::from(vec![
            Span::styled("── ", Style::default().fg(theme.border)),
            Span::styled(
                label,
                Style::default()
                    .fg(theme.secondary_info)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!(" {rule}"), Style::default().fg(theme.border)),
        ])
    } else if let Some(file_info) = raw.strip_prefix("§§file:") {
        let rule = "─".repeat(45);
        Line::from(vec![
            Span::styled("── ", Style::default().fg(theme.border)),
            Span::styled(
                format!("{} ", theme::ICON_FILE),
                Style::default().fg(theme.accent),
            ),
            Span::styled(
                file_info,
                Style::default()
                    .fg(theme.header_title)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!(" {rule}"), Style::default().fg(theme.border)),
        ])
    } else if let Some(rest) = raw.strip_prefix("§§hunk:") {
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
    } else if let Some(hint) = raw.strip_prefix("§§hint:") {
        Line::from(Span::styled(
            hint,
            Style::default()
                .fg(theme.faint_hint)
                .add_modifier(Modifier::ITALIC),
        ))
    } else if raw.starts_with('+') && !raw.starts_with("+++") {
        Line::from(vec![
            Span::styled(
                "+",
                Style::default()
                    .fg(theme.success)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(&raw[1..], Style::default().fg(theme.success)),
        ])
    } else if raw.starts_with('-') && !raw.starts_with("---") {
        Line::from(vec![
            Span::styled(
                "-",
                Style::default()
                    .fg(theme.danger)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(&raw[1..], Style::default().fg(theme.danger)),
        ])
    } else if raw.contains('|') && (raw.contains('+') || raw.contains('-')) {
        let parts: Vec<&str> = raw.splitn(2, '|').collect();
        if parts.len() == 2 {
            let mut spans = vec![
                Span::styled(parts[0], Style::default().fg(theme.text)),
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
            Line::from(Span::styled(raw, Style::default().fg(theme.text)))
        }
    } else if raw.starts_with("@@") || raw.contains("file changed") || raw.contains("files changed")
    {
        Line::from(Span::styled(
            raw,
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ))
    } else if raw.starts_with("diff --git") || raw.starts_with("index ") {
        Line::from(Span::styled(
            raw,
            Style::default()
                .fg(theme.secondary_info)
                .add_modifier(Modifier::BOLD),
        ))
    } else if raw.starts_with("commit ") {
        Line::from(Span::styled(
            raw,
            Style::default()
                .fg(theme.selected)
                .add_modifier(Modifier::BOLD),
        ))
    } else if raw.starts_with("Author:") || raw.starts_with("Date:") {
        Line::from(Span::styled(raw, Style::default().fg(theme.neutral_text)))
    } else if raw.contains("(current)") {
        Line::from(Span::styled(
            raw,
            Style::default()
                .fg(theme.success)
                .add_modifier(Modifier::BOLD),
        ))
    } else {
        Line::from(Span::styled(raw, Style::default().fg(theme.text)))
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
}
