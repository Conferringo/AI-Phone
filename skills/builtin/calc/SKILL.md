---
name: calc
description: Create, calculate and save real LibreOffice Calc spreadsheets through the Linux GUI.
---

Use computer-use with the observed application_id `linux:libreoffice-calc`. The exercised baseline is LibreOffice 25.2.3 on Linux: a new spreadsheet, Chinese text, numeric cells, multiplication and SUM formulas, formula-bar verification and native ODS saving. Other formats and advanced spreadsheet features need their own checks.

Open File → New → Spreadsheet without replacing an existing document. Select the intended start cell before pasting tab-separated columns and newline-separated rows with computer_type. Pasting can open Text Import asynchronously: an unchanged frame does not justify pasting again. Observe the actual dialog, check separated columns and the preview, and enable formula evaluation when formulas are intended. Confirm the import once and inspect the resulting sheet.

Check a formula cell's formula bar as well as its displayed result. A number that merely looks correct does not prove a formula exists. Check totals and representative inputs before saving.

Use Ctrl+S for a new document or Ctrl+Shift+S for Save As. A full destination path can be entered in the filename field. Confirm the format selector is ODF Spreadsheet for native ODS. An extension alone does not convert a file. Never overwrite another document to make an exercise succeed.

Input pastes at the current caret; it does not replace existing field text automatically. Use Ctrl+A only after confirming focus is in the intended filename, name box or text field. Keep other documents open and unchanged. Reuse action-returned frames and current observation IDs. Report the saved artifact and important checks briefly; the ledger already contains the operations.
