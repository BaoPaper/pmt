# pmt

PromptManager TUI for browsing prompt templates, filling variables, and copying
the rendered result to the clipboard.

## Features

- Reads templates from `~/.config/pmt/prompts.md`
- Uses `## Title` as template name; content until next heading is the body
- Supports hierarchy with `/` in titles (TreeView)
- Form-based variable input with live preview
- Variables: `{name}` or `{name|description}`
- Random placeholders: `{random|"opt1" "opt2" ...}` with reroll
- Copy renders only the final output (shows a short status message)

## Install

Rust toolchain is required.

```bash
cargo build --release
```

Binary will be at `target/release/pmt` (or `pmt.exe` on Windows).

## Run

```bash
cargo run
```

## Prompt file format

Create `~/.config/pmt/prompts.md`:

```md
## Writing/Email/FollowUp
Write a polite follow-up email to {name|recipient} about {topic|subject}.

## Coding/Review/Checklist
Review the {area|component} and list {random|"security" "performance" "usability"} risks.
```

Rules:

- Each template starts with `## Title`
- Body is everything until the next `##`
- Leading whitespace is preserved
- If no template headings exist, the app shows an error

## Placeholders

- `{name}` or `{name|description}` creates an input field
- Empty input leaves the placeholder unchanged
- `{random|"opt1" "opt2" ...}` is rolled on load and kept consistent between
  preview and final copy

## Keybindings

List view:

- Up/Down or j/k: move
- Enter / double click: open template
- q: quit

Editor view:

- Tab or Up/Down: switch fields
- Ctrl+C: copy rendered output
- F5: reroll random placeholders
- Esc: back to list

## Notes

- Mouse capture is enabled to support double click in the list
