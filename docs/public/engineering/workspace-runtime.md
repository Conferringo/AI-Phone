# Workspace execution

`workspace_run` executes a task in the persistent tools container. It is separate
from the graphical desktop: browser, office and messaging interactions continue
to use the corresponding device tools. The harness workspace is mounted at
`/workspace`; its files are also available in the shared file manager.

## Ownership and execution

The Rust assistant prepares the container, resolves its installed native worker
and issues one `docker exec` for each task. Both the workspace and desktop images
include the same Rust `desktop-bridge` binary. Workspace subcommands run headlessly.

The worker owns a shell process group, its 90-second execution deadline and the
`.jobs/<UUID>.pid` and `.jobs/<UUID>.cancel` control files. It publishes a complete
PID file by rename, consumes cancellation requested before startup, and cleans up
the group and control files after completion, timeout or termination. Normal shell
exit codes are retained; timeout returns 124 and cancellation returns 130. Output
is streamed through Docker; the assistant retains at most 64 KiB while continuing
to drain the pipes. The host command has a 100-second deadline.

Tasks continue while device focus changes. The guest deadline remains active
if the assistant exits. Restarting the container or host ends the task.

## Cancellation and existing installations

Cancellation addresses the same unique job ID and uses a bounded 10-second
control command. If that command fails, the assistant reports that cancellation
is unconfirmed. Inspect existing workspace artifacts and task state before
retrying.

Existing tools containers retain their identity, installed tools, mounted files
and running tasks. If the worker contract is absent or outdated, the assistant
copies the native worker from the installed workspace image and replaces only
its reserved executable by rename. An executing binary keeps its old inode.
The assistant caches worker readiness by container ID.

The workspace image includes third-party task tools such as Python libraries.
Rust owns execution and cancellation.

## Maintainer verification

The native CLI integration tests cover output and artifacts, exit codes,
cancellation before and during execution, independent tasks, process-group
cleanup, termination signals, deadlines and malformed job IDs. The source CI
also runs the actual assistant against a disposable Docker container to verify
fresh preparation, in-place worker installation while an existing task runs,
file preservation and a failed cancellation receipt.
