# Desktop control location

`computer_find_control` reads real AT-SPI controls in the currently observed Linux foreground window. It accepts a name, a canonical role (for example `button`, `text` or `spin button`), or both. Results preserve duplicate names and expose screen bounds, enabled/editable/focused states and whether the node supports interaction. Inspection is read-only; it neither selects a candidate nor grants a new action lease.

The existing desktop observation carries an X11 window ID and process ID through protobuf and migration38. The worker validates the stored observation, device and control epoch before inspection. The native Rust reader matches the accessibility application to that process, requires a unique active window, and checks the foreground identity again after reading. It uses the existing action and observation owners; there is no additional Agent loop or mandatory scan after every action.

Traversal is bounded to384 nodes and12 levels. Hidden/defunct nodes are excluded; virtualized descendants are not expanded into unbounded table cells or file lists. `partial=true` means the entire window was not inspected, including when a virtualized branch was deliberately omitted. Use the existing OCR/crop tools when a window does not provide accessibility or the reported geometry does not match the screen.

## Field replacement

`computer_type` accepts optional `replace=true` to replace the focused editable field with one select-all/paste action. The default remains ordinary Unicode paste. The stored observation supplies the window/process identity; the native reader checks the current foreground window and real AT-SPI focus before any clipboard or keyboard input. Text entries, editable text and spin fields are eligible; document/canvas roles are excluded. This check runs only for replacement, not after every GUI action.

The existing protobuf action carries the flag. A distinct native opcode prevents an older guest from silently treating replacement as append. A focus rejection returns `not_executed` and is recorded as a failed action, distinct from an interrupted or unknown result. Reusing the same action ID returns that rejection without retrying input. Genuine unknown outcomes retain the existing observe-before-replay contract.

The exercised GTK Save dialog replaced Chinese and emoji exactly, then appended normally; clipboard copy through the UI confirmed the value. Canvas focus and a stale window ID rejected input without changing the clipboard. The dialog was cancelled and no document was saved. When accessibility focus is unavailable, inspect the current screenshot.

## Evidence and limits

A real isolated LibreOffice GTK3 process in the existing Linux device exposed its Save dialog. Inspection found the filename field as editable and focused, distinguished its same-name label, and returned unique Save/Cancel buttons. The returned Cancel coordinates were used for a real click; the modal closed and the original window returned. Three lookup samples took approximately79–88ms including the container invocation.

Application-reported geometry still requires visual confirmation. LibreOffice custom GTK3 controls reported a root-pane origin inconsistent with its native GTK host. The reader now derives the translation from the matching host allocation, before clipping to the screen; no fixed pixel offset or application name is used. Native GTK widgets keep their own coordinates. The corrected Save toolbar button opened the real modal in both a maximized window and a moved 950×650 window; native filename and Cancel controls remained correct. Those two custom-control queries took about148ms including container invocation. A complex main view can also reach the traversal bound. The initial and later ordinary Draw exercises remain failed, as recorded in [application guides](app-guides.md).

New desktop images include AT-SPI and LibreOffice GTK3, enable desktop accessibility, and select GTK3 for newly started applications. An already running LibreOffice process retains its selected UI backend. Existing documents and processes are not killed to force migration; the current device was qualified using a separate user profile. Save existing work and restart the application normally when adopting the new backend.

The native guest control channel still uses its existing Rust stdio contract; the browser/business observation path uses protobuf.
