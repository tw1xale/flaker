pub mod closure_diff;
pub mod confirm;
pub mod diff;
pub mod filter;
pub mod header;
pub mod input_modal;
pub mod menu;
pub mod pager;
pub mod result;
pub mod selective;

/// Slices a string based on terminal display column boundaries rather than Unicode scalar values.
///
/// Ensures that horizontal scrolling in input fields handles multi-column characters
/// (such as CJK ideographs, emojis, and full-width symbols) correctly without truncating
/// or dropping characters prematurely.
pub fn slice_by_column_width(text: &str, scroll_col: usize, max_col_width: usize) -> String {
    if max_col_width == 0 {
        return String::new();
    }

    let mut current_col: usize = 0;
    let mut visible = String::with_capacity(text.len());
    let mut visible_width: usize = 0;

    for ch in text.chars() {
        let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        let next_col = current_col.saturating_add(ch_width);

        if next_col <= scroll_col {
            current_col = next_col;
            continue;
        }

        if visible_width.saturating_add(ch_width) <= max_col_width {
            visible.push(ch);
            visible_width = visible_width.saturating_add(ch_width);
        } else {
            break;
        }

        current_col = next_col;
    }

    visible
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slice_by_column_width_ascii() {
        let text = "hello world";
        assert_eq!(slice_by_column_width(text, 0, 5), "hello");
        assert_eq!(slice_by_column_width(text, 6, 5), "world");
        assert_eq!(slice_by_column_width(text, 0, 20), "hello world");
    }

    #[test]
    fn test_slice_by_column_width_cjk() {
        // Each '界' has display width 2
        let text = "界".repeat(10);
        // max width 6 columns -> 3 characters
        assert_eq!(slice_by_column_width(&text, 0, 6), "界".repeat(3));
        // scroll 4 columns (2 chars), max 6 columns (3 chars) -> 3 characters
        assert_eq!(slice_by_column_width(&text, 4, 6), "界".repeat(3));
    }
}
