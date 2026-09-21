//! Command line definition and resolution against the terminal probe.

use std::path::PathBuf;

use clap::{Args as ClapArgs, ValueEnum};

use crate::options::{FrontMatter, Glyphs, LinkMode, RenderOptions, TableOverflow, defaults};
use crate::style::ColorLevel;
use crate::term::Probe;
use crate::wrap::SoftBreaks;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, ValueEnum)]
pub enum ColorMode {
    #[default]
    Auto,
    Always,
    Never,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, ValueEnum)]
pub enum LinkArg {
    #[default]
    Auto,
    Osc8,
    Inline,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, ValueEnum)]
pub enum BorderArg {
    #[default]
    Auto,
    Unicode,
    Ascii,
}

/// Options of `mdkt cat`.
#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Markdown files to render in order; reads stdin when omitted, or for "-".
    pub files: Vec<PathBuf>,

    /// Line width in columns; defaults to the terminal width.
    #[arg(short, long)]
    pub width: Option<usize>,

    /// Rendering of single newlines inside a paragraph.
    #[arg(long, value_enum, default_value_t)]
    pub soft_breaks: SoftBreaks,

    /// Colour and text attributes.
    #[arg(long, value_enum, default_value_t)]
    pub color: ColorMode,

    /// Link rendering.
    #[arg(long, value_enum, default_value_t)]
    pub links: LinkArg,

    /// Character set for bullets, rules and table borders.
    #[arg(long, value_enum, default_value_t)]
    pub borders: BorderArg,

    /// Colour theme for code blocks.
    #[arg(long, default_value = defaults::CODE_THEME)]
    pub theme: String,

    /// Print the available code themes and exit.
    #[arg(long)]
    pub list_themes: bool,

    /// Columns per tab in code blocks.
    #[arg(long, default_value_t = defaults::TAB_WIDTH)]
    pub tab_width: usize,

    /// Table cells wider than their column.
    #[arg(long, value_enum, default_value_t)]
    pub table_overflow: TableOverflow,

    /// Hide the `#` markers before headings.
    #[arg(long)]
    pub no_heading_marker: bool,

    /// YAML or TOML metadata block at the top of the document.
    #[arg(long, value_enum, default_value_t)]
    pub front_matter: FrontMatter,

    /// Hide the file name header printed when several files are rendered.
    #[arg(long)]
    pub no_header: bool,
}

/// Combines CLI flags with terminal capabilities into concrete options.
pub fn resolve(cli: &Args, probe: &Probe) -> RenderOptions {
    let width = cli
        .width
        .or(probe.width)
        .filter(|w| *w > 0)
        .unwrap_or(defaults::WIDTH);
    let color = match cli.color {
        ColorMode::Never => ColorLevel::None,
        ColorMode::Always => probe.color.max(ColorLevel::Ansi16),
        ColorMode::Auto if probe.no_color => ColorLevel::None,
        ColorMode::Auto if probe.is_tty => probe.color,
        // Piped into a pager from an interactive shell: keep colours for `less -R`.
        ColorMode::Auto if probe.stdout_is_pipe && probe.stderr_is_tty => probe.color,
        ColorMode::Auto => ColorLevel::None,
    };
    let links = match cli.links {
        LinkArg::None => LinkMode::None,
        LinkArg::Inline => LinkMode::Inline,
        LinkArg::Osc8 if color != ColorLevel::None => LinkMode::Osc8,
        LinkArg::Osc8 => LinkMode::Inline,
        LinkArg::Auto if color != ColorLevel::None && probe.osc8 => LinkMode::Osc8,
        LinkArg::Auto => LinkMode::Inline,
    };
    let glyphs = match cli.borders {
        BorderArg::Unicode => Glyphs::unicode(),
        BorderArg::Ascii => Glyphs::ascii(),
        BorderArg::Auto if probe.utf8 => Glyphs::unicode(),
        BorderArg::Auto => Glyphs::ascii(),
    };
    RenderOptions {
        width,
        color,
        links,
        soft_breaks: cli.soft_breaks,
        code_theme: cli.theme.clone(),
        glyphs,
        tab_width: cli.tab_width,
        table_overflow: cli.table_overflow,
        heading_marker: !cli.no_heading_marker,
        front_matter: cli.front_matter,
        ..RenderOptions::default()
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    #[derive(Parser)]
    struct Wrap {
        #[command(flatten)]
        args: Args,
    }

    fn probe(is_tty: bool) -> Probe {
        Probe {
            is_tty,
            stdout_is_pipe: false,
            stderr_is_tty: is_tty,
            width: Some(100),
            color: ColorLevel::TrueColor,
            no_color: false,
            utf8: true,
            osc8: true,
        }
    }

    fn cli(args: &[&str]) -> Args {
        Wrap::parse_from(std::iter::once("mdkt").chain(args.iter().copied())).args
    }

    #[test]
    fn auto_color_kept_when_piped_from_a_terminal() {
        let mut p = probe(false);
        p.stdout_is_pipe = true;
        p.stderr_is_tty = true;
        assert_eq!(resolve(&cli(&[]), &p).color, ColorLevel::TrueColor);
        p.stdout_is_pipe = false;
        assert_eq!(resolve(&cli(&[]), &p).color, ColorLevel::None);
        p.stdout_is_pipe = true;
        p.stderr_is_tty = false;
        assert_eq!(resolve(&cli(&[]), &p).color, ColorLevel::None);
    }

    #[test]
    fn auto_color_off_when_piped() {
        let o = resolve(&cli(&[]), &probe(false));
        assert_eq!(o.color, ColorLevel::None);
        assert_eq!(o.links, LinkMode::Inline);
        assert_eq!(o.width, 100);
    }

    #[test]
    fn always_forces_color_and_osc8_follows() {
        let o = resolve(&cli(&["--color", "always"]), &probe(false));
        assert_eq!(o.color, ColorLevel::TrueColor);
        assert_eq!(o.links, LinkMode::Osc8);
    }

    #[test]
    fn no_color_env_wins_over_auto() {
        let mut p = probe(true);
        p.no_color = true;
        assert_eq!(resolve(&cli(&[]), &p).color, ColorLevel::None);
    }

    #[test]
    fn front_matter_hidden_by_default() {
        assert_eq!(
            resolve(&cli(&[]), &probe(true)).front_matter,
            FrontMatter::Hide
        );
        assert_eq!(
            resolve(&cli(&["--front-matter", "show"]), &probe(true)).front_matter,
            FrontMatter::Show
        );
    }

    #[test]
    fn explicit_width_and_soft_breaks() {
        let o = resolve(
            &cli(&["-w", "30", "--soft-breaks", "preserve"]),
            &probe(true),
        );
        assert_eq!(o.width, 30);
        assert_eq!(o.soft_breaks, SoftBreaks::Preserve);
    }
}
