# mdkt lint

Checks Markdown links, anchors and heading structure. No network access: only what is in the file
tree is verified.

```
mdkt lint README.md
mdkt lint docs/                 # walks the directory for .md files
mdkt lint --format json docs/   # one JSON object per finding
```

Findings are printed as `file:line:column: rule: message` and the exit code is 1 when there is
any. `--list-rules` prints the rules with their default state.

| Rule                | Default | Check                                                                |
| ------------------- | ------- | -------------------------------------------------------------------- |
| `link-file`         | on      | a relative link or image target that does not exist                  |
| `link-anchor`       | on      | a `#anchor` that matches no heading, in this file or in a linked one |
| `link-ref`          | on      | a reference link with no definition, and a definition nobody uses    |
| `heading-skip`      | on      | a heading level that jumps by more than one                          |
| `heading-dup`       | on      | two headings that produce the same anchor                            |
| `image-alt`         | on      | an image with no alt text                                            |
| `heading-single-h1` | off     | more than one level one heading                                      |
| `fence-lang`        | off     | a fenced code block with no language                                 |
| `trailing-space`    | off     | trailing whitespace that is not a hard break                         |
| `line-length`       | off     | a line over `--line-length`, code and URLs excluded                  |

Turn rules off with `--disable RULE` and on with `--enable RULE`, repeating either.

Anchors are the ones GitHub computes, so a link that passes here works on GitHub. Explicit `id`
and `name` attributes in HTML count as anchors too.
