# Desktop control location

`computer_find_control` reads real AT-SPI controls in the currently observed Linux foreground window. It accepts a name, a canonical role (for example `button`, `text` or `spin button`), or both. Results preserve duplicate names and expose the provider's bounds, enabled/editable/focused states and whether the node supports interaction. Inspection is read-only; it neither selects a candidate nor grants a new action lease.

The existing desktop observation carries an X11 window ID and process ID through protobuf and migration38. The worker validates the stored observation, device and control epoch before inspection. The native Rust reader matches the accessibility application to that process, requires a unique active window, and checks the foreground identity again after reading. It uses the existing action and observation owners; there is no additional Agent loop or mandatory scan after every action.

Traversal is bounded to384 nodes and12 levels. Hidden/defunct nodes are excluded; virtualized descendants are not expanded into unbounded table cells or file lists. `partial=true` means the entire window was not inspected, including when a virtualized branch was deliberately omitted. An empty partial result does not prove that a control is absent. Use the existing OCR/crop tools when a window does not provide accessibility or the reported geometry does not match the screen.

## Evidence and limits

A real isolated LibreOffice GTK3 process in the existing Linux device exposed its Save dialog. Inspection found the filename field as editable and focused, distinguished its same-name label, and returned unique Save/Cancel buttons. The returned Cancel coordinates were used for a real click; the modal closed and the original window returned. Three lookup samples took approximately79–88ms including the container invocation. These are individual samples, not an overall latency benchmark or a general drawing qualification.

Application-reported geometry still requires visual confirmation. In the exercised LibreOffice main view, custom toolbar positions could differ from their rendered positions; those controls are not certified for blind clicking. A complex main view can also reach the traversal bound. The initial and later ordinary Draw exercises remain failed, as recorded in [application guides](app-guides.md).

New desktop images include AT-SPI and LibreOffice GTK3, enable desktop accessibility, and select GTK3 for newly started applications. An already running LibreOffice process retains its selected UI backend. Existing documents and processes are not killed to force migration; the current device was qualified using a separate user profile. Save existing work and restart the application normally when adopting the new backend.

The native guest control channel still uses its existing Rust stdio contract; the browser/business observation path uses protobuf. This change does not complete the remaining runtime/protocol migration or qualify all desktop applications. Errors and control results must not be presented as successful GUI actions.
