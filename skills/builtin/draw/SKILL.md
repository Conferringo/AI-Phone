---
name: draw
description: Draw labeled shapes and arrow connections in real LibreOffice Draw; avoid repeated icon guessing.
---

Use computer-use with the observed application_id `linux:libreoffice-draw`. These methods have been exercised in LibreOffice 25.2.3, but successful actions and an existing ODG do not prove a correct drawing. Check the actual shape count, labels, connections and page bounds before finishing.

Identify the outline rectangle tool from the current toolbar, not from remembered coordinates or a different shape-group icon. Draw one visible shape inside the white page and confirm its outline before adding more. With the shape selected, F2 enters its text; Escape leaves text editing. Only use Ctrl+A to replace a label after confirming text-edit focus. On the canvas it selects objects instead.

Escape can leave the Text Box tool active. Before editing another shape, choose the selection arrow and click its filled area; confirm the shape's selection handles, then press F2. Clicking a label while Text Box is active can create a separate text frame over the original label. Undo that insertion and select the original shape rather than adding more text.

For repeated equal shapes, Shift+F3 opens Duplicate. Choose the copy count and horizontal displacement from the actual page and shape sizes; keep vertical displacement zero for a row. Check that the last copy stays inside the page. Tab can cycle selected objects; enter text mode to replace each copy's label. This is more reliable than repeatedly redrawing and repairing every object's dimensions.

Set the first shape's geometry before duplicating it. Copies already share its dimensions; do not repeatedly resize or reposition them when the drawing already meets the request. Save a new ODG in the task directory early, then save subsequent edits to it.

F4 and the Position and Size sidebar expose geometry. Read the displayed unit. Focus the actual numeric field, then use `computer_type(replace=true)` to replace its value atomically. Do not split this into Ctrl+A and a normal paste: a successful shortcut does not prove the old number is selected, and another decimal can be appended. If the tool rejects field focus, locate the editable control again instead of falling back to append mode. Check the displayed value before confirming the dialog. Do not repeat a failed drag or tool selection without checking the new frame.

Use an arrow-line tool for the connection. Confirm its direction and endpoints. An existing arrow can be copied and its position adjusted for another gap. When replacing a mistaken connector, remove the old segment rather than leaving both versions. Inspect each gap at a readable scale for extra lines crossing a box or label, and inspect the whole page before saving the final ODG. Never claim the drawing succeeded from successful clicks alone.

The menu path Shape → Insert → Lines and Arrows → Line Ends with Arrow selects a directional line without guessing toolbar groups. Drag from the source toward the destination, check the arrowhead, and select the tool again for the next connection. Save the ODG with Ctrl+S; the PDF toolbar button opens an export dialog. Confirm its folder and PDF format before exporting, then use device_file_list to check both files and device_file_manage to copy the actual saved output.

Documented shortcut references, still requiring current-window verification: [Draw keys](https://help.libreoffice.org/latest/en-GB/text/sdraw/04/01020000.html?DbPAR=DRAW), [Duplicate](https://help.libreoffice.org/latest/en-US/text/simpress/01/02120000.html?DbPAR=DRAW&System=WIN).
