# mdkt

Markdown tools for the terminal in one binary, sharing one parser and one model.

| Command                   | Purpose                                    |
| ------------------------- | ------------------------------------------ |
| [mdkt cat](docs/cat.md)   | render a document as styled text           |
| [mdkt fmt](docs/fmt.md)   | rewrap prose and normalise block syntax    |
| [mdkt toc](docs/toc.md)   | insert or refresh a table of contents      |
| [mdkt lint](docs/lint.md) | check links, anchors and heading structure |

```
cargo build --release
cargo install --path .
```

Each command reads files or stdin and writes to stdout.

## Shell integration

```
mdkt --completions bash > ~/.local/share/bash-completion/completions/mdkt
mdkt --completions zsh > ~/.local/share/zsh/site-functions/_mdkt
mdkt --man > mdkt.1
```

The manual holds one page for `mdkt` and one for each command.

## Why

Renderers reflow a paragraph the way CommonMark specifies, which shows every line break of a
hand-wrapped source as a break on screen. `cat` joins them by default and keeps them on request,
with continuation lines aligned under their list item or quote. `fmt` solves the same problem at
the source: it rewraps the file so the breaks land where you want them.

## The library

The crate also holds the parser, the block model, the terminal probe, the ANSI writer, GitHub
anchors, and a ranged syntax tree that keeps the byte range of every construct for the commands
that rewrite or locate Markdown rather than render it.

```
cargo test
cargo clippy --all-targets
```

`UPDATE_FIXTURES=1 cargo test` rewrites the expected outputs under `tests/fixtures` after an
intentional change.
