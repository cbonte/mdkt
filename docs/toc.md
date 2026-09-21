# mdkt toc

Inserts or refreshes a table of contents in a Markdown file.

```
mdkt toc README.md              # print the table, or the updated document
mdkt toc --write README.md      # update the file in place
mdkt toc --check docs/*.md      # exit 1 if a table is stale
```

The table is written between two HTML comments, which you add once where it belongs:

```
<!-- toc -->
<!-- /toc -->
```

Without those markers, `mdkt toc` prints the table alone so you can paste it.

| Option          | Default | Effect                                                   |
| --------------- | ------- | -------------------------------------------------------- |
| `--min-depth N` | 2       | shallowest heading level listed, so the title is skipped |
| `--max-depth N` | 4       | deepest heading level listed                             |
| `--bullet C`    | `-`     | list marker                                              |
| `--marker NAME` | `toc`   | name used in the comment markers                         |
| `--write`       | off     | update the files in place                                |
| `--check`       | off     | print the stale files and exit 1                         |

Anchors are the ones GitHub computes, duplicates numbered the same way, so the links work on
GitHub and with `mdkt cat --section`.
