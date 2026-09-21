# mdkt fmt

Reformats Markdown: rewraps prose to a width and normalises block syntax. Markup is never
rewritten, only moved.

```
mdkt fmt README.md                 # to stdout
mdkt fmt --write docs/*.md         # in place
mdkt fmt --check docs/*.md         # exit 1 if a file would change
mdkt fmt --diff README.md          # show what would change
mdkt fmt --width 0 notes.md        # one line per paragraph
```

| Option          | Default | Effect                                    |
| --------------- | ------- | ----------------------------------------- |
| `--width N`     | 80      | target line width; 0 means no wrapping    |
| `--bullet C`    | `-`     | marker for unordered lists                |
| `--write`, `-i` | off     | rewrite the files in place                |
| `--check`       | off     | print the files that would change, exit 1 |
| `--diff`        | off     | print the changed lines                   |

## What it changes

Paragraphs are rewrapped, so hand-wrapped source lines are joined and refilled. Headings become
ATX with one space. List markers are normalised, ordered lists renumbered from their start,
continuation lines aligned under the item text. Block quotes use `> `. Tables are realigned with
padded cells. Blank lines between blocks are reduced to one, and tight lists stay tight.

## What it never changes

Emphasis, links and their destinations, code spans, escapes, entities, inline HTML and footnote
references are copied from the source byte for byte. Code blocks, HTML blocks, front matter and
link reference definitions are kept line for line.

A rewrapped line can never start a new block: a word that would open one, such as a dash or `1.`,
is kept on the same line as the word before it, rather than escaped.

Three properties are tested over the whole fixture corpus: formatting twice gives the same result,
the parser event stream is unchanged apart from line breaks, and no line exceeds the width unless
one unbreakable run does.
