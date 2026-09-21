//! Walks the block tree, applies container prefixes and writes wrapped lines.

use std::io::{self, Write};

use crate::ansi::AnsiWriter;
use crate::highlight::{Highlighter, plain_lines};
use crate::model::{AlertKind, Block, InlineText, Line, ListItem, Segment};
use crate::options::{FrontMatter, RenderOptions};
use crate::style::Style;
use crate::table;
use crate::wrap::{WrapOptions, display_width, segments_width, wrap};

/// Text written before every line of a container. `first` goes on the container's first
/// line, `rest` on the following ones; both have the same display width.
struct Prefix {
    first: Vec<Segment>,
    rest: Vec<Segment>,
    width: usize,
    used: bool,
}

impl Prefix {
    fn new(first: Vec<Segment>, rest: Vec<Segment>) -> Prefix {
        let width = segments_width(&first);
        debug_assert_eq!(width, segments_width(&rest));
        Prefix {
            first,
            rest,
            width,
            used: false,
        }
    }

    fn same(segs: Vec<Segment>) -> Prefix {
        Prefix::new(segs.clone(), segs)
    }
}

pub struct Renderer<'a, W: Write> {
    out: AnsiWriter<W>,
    opts: &'a RenderOptions,
    hl: Option<&'a Highlighter>,
    prefixes: Vec<Prefix>,
    blank_pending: bool,
    started: bool,
}

impl<'a, W: Write> Renderer<'a, W> {
    pub fn new(out: AnsiWriter<W>, opts: &'a RenderOptions, hl: Option<&'a Highlighter>) -> Self {
        Renderer {
            out,
            opts,
            hl,
            prefixes: Vec::new(),
            blank_pending: false,
            started: false,
        }
    }

    pub fn finish(mut self) -> io::Result<()> {
        self.out.flush()
    }

    fn available_width(&self) -> usize {
        let used: usize = self.prefixes.iter().map(|p| p.width).sum();
        self.opts
            .width
            .saturating_sub(used)
            .max(self.opts.min_content_width)
    }

    fn write_prefixes(&mut self) -> io::Result<()> {
        for p in &mut self.prefixes {
            let segs = if p.used { &p.rest } else { &p.first };
            p.used = true;
            self.out.write_segments(segs)?;
        }
        Ok(())
    }

    fn emit_blank(&mut self) -> io::Result<()> {
        let mut segs: Vec<Segment> = self
            .prefixes
            .iter()
            .flat_map(|p| p.rest.iter().cloned())
            .collect();
        while let Some(last) = segs.last_mut() {
            let trimmed = last.text.trim_end();
            if trimmed.is_empty() {
                segs.pop();
            } else {
                last.text.truncate(trimmed.len());
                break;
            }
        }
        self.out.write_segments(&segs)?;
        self.out.end_line()
    }

    fn flush_blank(&mut self) -> io::Result<()> {
        if self.blank_pending && self.started {
            self.emit_blank()?;
        }
        self.blank_pending = false;
        Ok(())
    }

    fn emit_line(&mut self, line: &Line) -> io::Result<()> {
        self.flush_blank()?;
        self.write_prefixes()?;
        self.out.write_segments(line)?;
        self.out.end_line()?;
        self.started = true;
        Ok(())
    }

    fn with_prefix<R>(
        &mut self,
        prefix: Prefix,
        f: impl FnOnce(&mut Self) -> io::Result<R>,
    ) -> io::Result<R> {
        self.flush_blank()?;
        self.prefixes.push(prefix);
        let result = f(self);
        let prefix = self.prefixes.pop().expect("pushed above");
        if !prefix.used {
            self.prefixes.push(prefix);
            self.emit_line(&Vec::new())?;
            self.prefixes.pop();
        }
        result
    }

    fn emit_wrapped(&mut self, text: &InlineText) -> io::Result<()> {
        let lines = wrap(
            text,
            &WrapOptions {
                width: self.available_width(),
                soft_breaks: self.opts.soft_breaks,
            },
        );
        for line in &lines {
            self.emit_line(line)?;
        }
        Ok(())
    }

    /// Renders sibling blocks; `tight` suppresses the blank line between them.
    pub fn render_blocks(&mut self, blocks: &[Block], tight: bool) -> io::Result<()> {
        for (i, block) in blocks.iter().enumerate() {
            if i > 0 && !tight {
                self.blank_pending = true;
            }
            self.render_block(block)?;
        }
        Ok(())
    }

    fn render_block(&mut self, block: &Block) -> io::Result<()> {
        match block {
            Block::Paragraph(text) => self.emit_wrapped(text),
            Block::Heading { level, content } => self.render_heading(*level, content),
            Block::CodeBlock { lang, code } => self.render_code(lang.as_deref(), code),
            Block::BlockQuote { kind, blocks } => self.render_quote(*kind, blocks),
            Block::List {
                start,
                tight,
                items,
            } => self.render_list(*start, *tight, items),
            Block::Table(t) => {
                let lines = table::render(t, self.available_width(), self.opts);
                for line in &lines {
                    self.emit_line(line)?;
                }
                Ok(())
            }
            Block::Rule => {
                let width = self.available_width();
                let rule = self.opts.glyphs.rule.to_string().repeat(width);
                self.emit_line(&vec![Segment::new(rule, self.opts.theme.rule)])
            }
            Block::FrontMatter { kind, text } => match self.opts.front_matter {
                FrontMatter::Hide => Ok(()),
                FrontMatter::Show => self.render_code(Some(kind.language()), text),
            },
            Block::Html(html) => {
                for line in plain_lines(html, self.opts.theme.html) {
                    self.emit_line(&line)?;
                }
                Ok(())
            }
        }
    }

    fn render_quote(&mut self, kind: Option<AlertKind>, blocks: &[Block]) -> io::Result<()> {
        let style = match kind {
            Some(kind) => self.opts.theme.alert[kind.index()],
            None => self.opts.theme.quote_marker,
        };
        let marker = Segment::new(self.opts.glyphs.quote_marker.clone(), style);
        self.with_prefix(Prefix::same(vec![marker]), |r| {
            if let Some(kind) = kind {
                let label = vec![Segment::new(kind.label(), style.bold())];
                r.emit_line(&label)?;
                r.blank_pending = true;
            }
            r.render_blocks(blocks, false)
        })
    }

    fn render_heading(&mut self, level: u8, content: &InlineText) -> io::Result<()> {
        self.blank_pending = true;
        if self.opts.heading_marker {
            let marker = format!("{} ", "#".repeat(usize::from(level)));
            let pad = " ".repeat(display_width(&marker));
            let prefix = Prefix::new(
                vec![Segment::new(marker, self.opts.theme.heading_marker)],
                vec![Segment::new(pad, Style::PLAIN)],
            );
            self.with_prefix(prefix, |r| r.emit_wrapped(content))?;
        } else {
            self.emit_wrapped(content)?;
        }
        self.blank_pending = true;
        Ok(())
    }

    fn expand_tabs(&self, code: &str) -> String {
        let tab = self.opts.tab_width.max(1);
        let mut out = String::with_capacity(code.len());
        let mut col = 0;
        for ch in code.chars() {
            match ch {
                '\t' => {
                    let n = tab - col % tab;
                    out.extend(std::iter::repeat_n(' ', n));
                    col += n;
                }
                '\n' => {
                    out.push('\n');
                    col = 0;
                }
                _ => {
                    out.push(ch);
                    col += display_width(ch.encode_utf8(&mut [0; 4]));
                }
            }
        }
        out
    }

    fn render_code(&mut self, lang: Option<&str>, code: &str) -> io::Result<()> {
        let code = self.expand_tabs(code);
        let lines = match self.hl {
            Some(hl) => hl.highlight(lang, &code),
            None => plain_lines(&code, self.opts.theme.code_block),
        };
        let indent = " ".repeat(self.opts.code_indent);
        let prefix = Prefix::same(vec![Segment::new(indent, Style::PLAIN)]);
        self.with_prefix(prefix, |r| {
            for line in &lines {
                r.emit_line(line)?;
            }
            Ok(())
        })
    }

    fn render_list(
        &mut self,
        start: Option<u64>,
        tight: bool,
        items: &[ListItem],
    ) -> io::Result<()> {
        let last = start.map(|s| s + items.len().saturating_sub(1) as u64);
        let number_width = last.map(|n| n.to_string().len()).unwrap_or(0);
        for (i, item) in items.iter().enumerate() {
            if i > 0 && !tight {
                self.blank_pending = true;
            }
            let marker = match start {
                Some(s) => format!("{:>number_width$}. ", s + i as u64),
                None => self.opts.glyphs.bullet.clone(),
            };
            let mut first = vec![Segment::new(marker.clone(), self.opts.theme.list_marker)];
            let mut width = display_width(&marker);
            if let Some(done) = item.task {
                let (text, style) = if done {
                    (
                        self.opts.glyphs.task_done.clone(),
                        self.opts.theme.task_done,
                    )
                } else {
                    (
                        self.opts.glyphs.task_todo.clone(),
                        self.opts.theme.list_marker,
                    )
                };
                width += display_width(&text);
                first.push(Segment::new(text, style));
            }
            let rest = vec![Segment::new(" ".repeat(width), Style::PLAIN)];
            self.with_prefix(Prefix::new(first, rest), |r| {
                r.render_blocks(&item.blocks, tight)
            })?;
        }
        Ok(())
    }
}
