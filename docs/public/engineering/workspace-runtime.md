# Workspace execution

`workspace_run` executes a task in the persistent tools container. It is separate
from the graphical desktop: browser, office and messaging interactions continue
to use the corresponding device tools. The harness workspace is mounted at
`/workspace`; its files are also available in the shared file manager.

## Ownership and execution

The Rust assistant prepares the container, resolves its installed native worker
and issues one `docker exec` for each task. Both the workspace and desktop images
include the same Rust `desktop-bridge` binary. The workspace subcommands do not
start a display server or load desktop configuration.

The worker owns a shell process group, its 90-second execution deadline and the
`.jobs/<UUID>.pid` and `.jobs/<UUID>.cancel` control files. It publishes a complete
PID file by rename, consumes cancellation requested before startup, and cleans up
the group and control files after completion, timeout or termination. Normal shell
exit codes are retained; timeout returns 124 and cancellation returns 130. Output
is streamed through Docker; the assistant retains at most 64 KiB while continuing
to drain the pipes. The host command has a 100-second deadline.

Changing the focused device in the UI does not cancel the task. Closing the
assistant does not remove the worker's guest-side deadline. This contract does
not provide durable job resumption after a container or host restart.

## Cancellation and existing installations

Cancellation addresses the same unique job ID and uses a bounded 10-second
control command. If that command fails, the assistant reports that cancellation
is unconfirmed and the result may be unknown; it does not claim that the task was
stopped or automatically replay it. Existing artifacts remain in the workspace.

Existing tools containers retain their identity, installed tools, mounted files
and running tasks. If the worker contract is absent or outdated, the assistant
copies the native worker from the installed workspace image and replaces only
its reserved executable by rename. An executing binary keeps its old inode.
There is no Python execution fallback. Readiness is cached against the actual
container ID, so subsequent tasks do not repeat the version probe.

The workspace image also carries third-party tools for user tasks, including
Python libraries. Those tools are distinct from the first-party Rust execution
and cancellation implementation; their presence must not be described as a
Python-free guest system.

## Maintainer verification

The native CLI integration tests cover output and artifacts, exit codes,
cancellation before and during execution, independent tasks, process-group
cleanup, termination signals, deadlines and malformed job IDs. The source CI
also runs the actual assistant against a disposable Docker container to verify
fresh preparation, in-place worker installation while an existing task runs,
file preservation and a failed cancellation receipt. Its minimal test image
does not contain Python. These checks do not replace full installer and
end-to-end device qualification.
