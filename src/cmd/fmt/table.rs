//! Rewriting a GFM table with aligned pipes.

use crate::tree::{Kind, Node};
use crate::wrap::display_width;
use pulldown_cmark::Alignment;

use crate::cmd::fmt::options::FmtOptions;

/// Narrowest alignment row a column can carry, as in `:-:`.
const MIN_RULE: usize = 3;

/// Cells of a row, each trimmed of its surrounding spaces.
fn cells<'s>(src: &'s str, row: &Node<'_>) -> Vec<&'s str> {
    row.children
        .iter()
        .filter(|c| matches!(c.kind, Kind::TableCell))
        .map(|c| src[c.range.clone()].trim())
        .collect()
}

/// Head and body rows of a table.
fn rows<'s>(src: &'s str, table: &Node<'_>) -> (Vec<&'s str>, Vec<Vec<&'s str>>) {
    let mut head = Vec::new();
    let mut body = Vec::new();
    for child in &table.children {
        match child.kind {
            Kind::TableHead => head = cells(src, child),
            Kind::TableRow => body.push(cells(src, child)),
            _ => {}
        }
    }
    (head, body)
}

/// Alignment row for one column, such as `:---` or `---:`.
fn rule(align: Alignment, width: usize) -> String {
    let width = width.max(MIN_RULE);
    match align {
        Alignment::Left => format!(":{}", "-".repeat(width - 1)),
        Alignment::Right => format!("{}:", "-".repeat(width - 1)),
        Alignment::Center => format!(":{}:", "-".repeat(width - 2)),
        Alignment::None => "-".repeat(width),
    }
}

/// Pads `cell` to `width` columns according to its alignment.
fn pad(cell: &str, width: usize, align: Alignment) -> String {
    let w = display_width(cell);
    if w >= width {
        return cell.to_string();
    }
    let total = width - w;
    match align {
        Alignment::Right => format!("{}{cell}", " ".repeat(total)),
        Alignment::Center => {
            let left = total / 2;
            format!("{}{cell}{}", " ".repeat(left), " ".repeat(total - left))
        }
        _ => format!("{cell}{}", " ".repeat(total)),
    }
}

/// Formats the table as lines, one per source row plus the alignment row.
pub fn render(src: &str, table: &Node<'_>, align: &[Alignment], opts: &FmtOptions) -> Vec<String> {
    let (head, body) = rows(src, table);
    let columns = head
        .len()
        .max(body.iter().map(Vec::len).max().unwrap_or(0))
        .max(align.len());
    if columns == 0 {
        return Vec::new();
    }
    let mut widths = vec![MIN_RULE; columns];
    for row in std::iter::once(&head).chain(body.iter()) {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(display_width(cell));
        }
    }
    let alignment = |i: usize| align.get(i).copied().unwrap_or(Alignment::None);
    let pad_str = " ".repeat(opts.table_padding);
    let line = |cells: &dyn Fn(usize) -> String| -> String {
        let mut out = String::from("|");
        for i in 0..columns {
            out.push_str(&pad_str);
            out.push_str(&cells(i));
            out.push_str(&pad_str);
            out.push('|');
        }
        out
    };
    let mut out = vec![line(&|i| {
        pad(head.get(i).copied().unwrap_or(""), widths[i], alignment(i))
    })];
    out.push(line(&|i| rule(alignment(i), widths[i])));
    for row in &body {
        out.push(line(&|i| {
            pad(row.get(i).copied().unwrap_or(""), widths[i], alignment(i))
        }));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree;

    fn format_table(src: &str) -> Vec<String> {
        let parsed = tree::parse(src);
        let node = parsed
            .children
            .iter()
            .find(|c| matches!(c.kind, Kind::Table(_)))
            .expect("a table");
        let Kind::Table(align) = &node.kind else {
            unreachable!()
        };
        render(src, node, align, &FmtOptions::default())
    }

    #[test]
    fn columns_are_padded_to_the_widest_cell() {
        let src = "|a|bbbb|\n|-|-|\n|cccc|d|\n";
        assert_eq!(
            format_table(src),
            ["| a    | bbbb |", "| ---- | ---- |", "| cccc | d    |"]
        );
    }

    #[test]
    fn alignment_is_kept_and_applied() {
        let src = "| l | c | r |\n|:--|:-:|--:|\n| 1 | 2 | 3 |\n";
        assert_eq!(
            format_table(src),
            [
                "| l   |  c  |   r |",
                "| :-- | :-: | --: |",
                "| 1   |  2  |   3 |"
            ]
        );
    }

    #[test]
    fn short_rows_are_padded_out() {
        let src = "| a | b | c |\n|---|---|---|\n| 1 |\n";
        // Three dashes is the narrowest alignment row, so columns never go below it.
        assert_eq!(
            format_table(src),
            [
                "| a   | b   | c   |",
                "| --- | --- | --- |",
                "| 1   |     |     |"
            ]
        );
    }
}
