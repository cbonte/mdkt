# mdkt cat

Renders Markdown as styled text on the terminal.

```
mdkt cat README.md
mdkt cat docs/*.md | less -R
cat notes.md | mdkt cat
mdkt cat --soft-breaks preserve notes.md | less -R
```

With several files, each document starts with its file name and a rule (`--no-header` hides it).
An unreadable file is reported on stderr, the others are still rendered, and the exit code is
non-zero.

## Behaviour

| Feature                           | Default                                                                                                                                                       | Option                                           |
| --------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------ |
| Single newline inside a paragraph | joined, paragraph reflowed (CommonMark)                                                                                                                       | `--soft-breaks preserve` keeps each source line  |
| Line width                        | terminal width, else 80                                                                                                                                       | `--width N`                                      |
| Colours                           | on when stdout is a terminal or a pipe from an interactive shell (`\| less -R`), off when redirected to a file or run without a terminal, off with `NO_COLOR` | `--color auto`, `always`, `never`                |
| Links                             | OSC 8 hyperlink on terminals that support it, else `text (url)`                                                                                               | `--links auto`, `osc8`, `inline`, `none`         |
| Bullets, rules, table borders     | Unicode in a UTF-8 locale, else ASCII                                                                                                                         | `--borders auto`, `unicode`, `ascii`             |
| Code blocks                       | syntax highlighted (syntect), tabs expanded                                                                                                                   | `--theme NAME`, `--list-themes`, `--tab-width N` |
| Tables                            | column widths fitted to the line, cells wrapped                                                                                                               | `--table-overflow wrap`, `truncate`              |
| Headings                          | `#` markers kept                                                                                                                                              | `--no-heading-marker`                            |
| Front matter                      | YAML or TOML metadata block hidden                                                                                                                            | `--front-matter show`                            |
| Outline                           | off                                                                                                                                                           | `--toc`, `--toc-depth N`                         |
| Section                           | whole document                                                                                                                                                | `--section HEADING` by text or by anchor         |
| Pager                             | off; `-p` sends the output to `MDKT_PAGER`, `PAGER`, else `less -R -F`                                                                                       | `-p`, `--pager`                                  |

Continuation lines inside lists and block quotes stay aligned under the item text in both
soft-break modes.

Supported: headings, paragraphs, emphasis, strong, strikethrough, inline code, block quotes,
GitHub alerts, ordered and unordered lists, task lists, fenced and indented code blocks, GFM
tables, thematic breaks, hard breaks, links, images (alt text and URL), footnotes.

Footnote references are numbered in order of first appearance; the bodies are printed as a
numbered list under a rule at the end of the document.

HTML is rendered the way GitHub displays it, for the tags GitHub keeps: `b`, `strong`, `i`, `em`,
`u`, `s`, `del`, `code`, `kbd`, `sub`, `sup`, `br`, `a`, `img`, `p`, `div`, `center`, `h1` to
`h6`, `blockquote`, `ul`, `ol`, `li`, `details`, `summary`, `table`, `pre`, `hr`. Comments,
`script` and `style` are dropped, entities are decoded, other tags are ignored and their text
kept. `align` attributes are honoured on table cells only.
