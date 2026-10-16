# Unreleased

## Added

- cat: draw Mermaid flowcharts as text.
- cat: draw Mermaid rounded and diamond nodes and multi-line labels.
- cat: draw Mermaid edge labels and dotted and thick edges.

# 0.1.0 - 2026-10-16

## Added

- cat: render a Markdown file as styled text on the terminal.
- cat: render several files in order with a file name header on each.
- cat: join the lines of a paragraph by default and keep them on request.
- cat: highlight the syntax of code blocks.
- cat: fit table columns to the line width.
- cat: render GitHub alerts such as NOTE and WARNING.
- cat: render footnotes as a numbered list at the end of the document.
- cat: hide the YAML or TOML front matter unless asked to show it.
- cat: send the output to a pager on request.
- mdkt: print a completion script for a given shell.
- mdkt: print the manual as one roff page per command.
- cat: list the outline of a document with the anchor of each heading.
- cat: render one section chosen by its heading text or its anchor.
- toc: insert or refresh a table of contents between two HTML comment markers.
- toc: report the files whose table of contents is stale.
- tree: keep the byte range of every construct.
- lint: report relative links and images that point to a missing file.
- lint: report anchors that match no heading in the file or in the linked one.
- lint: report skipped heading levels and duplicate headings.
- fmt: rewrap paragraphs to a target width without touching inline markup.
- fmt: normalise list markers and heading syntax and realign tables.
- fmt: report the files that would change or print a diff of the changes.
