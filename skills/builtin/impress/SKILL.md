---
name: impress
description: Build and verify real multi-slide LibreOffice Impress presentations with editable placeholders.
---

Use computer-use with the observed application_id `linux:libreoffice-impress`. The exercised baseline is LibreOffice 25.2.3 on Linux: three slides, Chinese title/subtitle/body text, slide insertion, native ODP saving, one genuine PPTX export and checking each saved slide. This does not certify arbitrary templates, animations, charts or other export formats.

Open File → New → Presentation. If a template chooser appears, dismiss it to work with a blank presentation. Preserve the other open documents.

Titles, subtitles and body placeholders are separate objects. Focus the actual placeholder before typing. A single click may select its frame without entering text editing; F2 enters text mode for the selected object. Confirm the caret or text selection before replacing content. Paste newline-separated points into a body placeholder, not into the slide title.

Ctrl+M adds a slide while Impress has focus. Select an appropriate title/body layout, then fill its placeholders. Escape leaves text editing; do not interpret a selected frame as an active text caret.

Save with Ctrl+S or Ctrl+Shift+S and confirm ODF Presentation for native ODP. Verify the saved filename, slide count and every slide's actual content by selecting its thumbnail. Do not claim a presentation is complete from one visible slide or by renaming its extension. Use current screenshots and keep the final report short; reusable experience must cite real observations and omit private filenames and content.
