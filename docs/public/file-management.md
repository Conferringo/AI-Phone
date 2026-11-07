# File management

The toolbar contains one cable toggle: grey is disconnected, black is connected. File management opens from the folder icon. Shared work files occupy the top pane, Android the lower left, and Linux the lower right. All panes use the same list, selection tools and path field.

- Open a folder or enter an absolute path. Android and Linux start at `/`; the work pane's `/` is the harness workspace, never the host filesystem root.
- Select files with the checkboxes. Copy, cut, paste, rename, delete, upload and create-folder actions stay in the pane. Keyboard shortcuts: Cmd/Ctrl+A, C, X, V and Delete. Deletion requires confirmation.
- Drag files or directories between panes to copy. Dropping onto a folder copies into that folder. Existing destinations are preserved; collisions return an error. A move deletes its source only after the copy succeeds.
- Browser uploads and chat attachments enter the workspace. A device download also leaves a work copy. Harness-created files already use the same workspace. Chat attachment originals remain separate from editable work copies.
- Direct phone–computer transfers require the cable. A reconnect changes its revision; an interrupted transfer is reported for inspection and is not automatically replayed. Cable access does not grant another Agent GUI control.

Lists include filename, size and last modification time. Directories can be copied recursively. Symbolic-link directories can be opened; copying special files or symbolic links is reported as unsupported. Directory download as a ZIP is not included.

## Implementation

The Rust Assistant owns `/v1/files/list`, `/v1/files/manage` and `/v1/files/upload`. `FileLocation`, `FileListing`, `FileManageRequest` and per-item `FileManageResult` are in `workbench.proto`. Ordinary requests and replies use protobuf; multipart uploads carry a protobuf `location` part followed by the file bytes. Browser download responses stream binary bytes.

Android uses the native Rust ADB client and SYNC streams, without a host SDK subprocess. Linux operations execute in the owned desktop container as guest root. Workspace paths resolve within the persistent workspace. Cross-device staging is private, file content streams without model calls or whole-file hashes, and a failed batch exposes each completed or failed item.

The Agent uses `device_file_list` and `device_file_manage` with the same implementation as the interface. Empty `device_id` selects the workspace. Guest access retains run/device binding checks. File operations are data-plane operations; browsing, social messages and office work still require the corresponding phone-use or computer-use tools.

## Verification scope

macOS development validation covers real Android and Linux guest roots, nested directories, Chinese filenames, binary round trips, collision refusal, rename/delete, authenticated protobuf operations and retained chats. Run isolated file tests with `cargo test -p ai-phone-assistant files::tests`. Run the explicit development-only real-device qualification with `cargo run -p ai-phone-assistant -- --qualify-file-manager-devices`; this command is excluded from release builds. Even `--all-features` unit tests do not contact installed devices. Windows host installation are not claimed as tested by this change.

---

# 文件管理

顶部数据线只有一个开关：灰色断开，黑色连接。文件窗口上方是工作文件，下方左侧手机、右侧电脑；三栏共用列表、多选、路径输入和图标工具栏。

打开目录或输入路径即可进入。拖拽默认复制，也可勾选后复制、剪切、粘贴、重命名、删除或新建文件夹。删除需要确认；同名不覆盖；移动在复制成功后才删除原文件。手机和电脑之间传输需连接数据线，工作文件与单台设备之间不需要。

新上传附件、设备取回的文件和 Harness 产物使用同一工作区。聊天原始附件与可编辑的工作副本分开保留。工作区 `/` 不代表宿主机根目录；手机和电脑则可以浏览各自根目录。列表显示名称、大小和修改时间。

接口与 Agent 共用 Rust 实现，普通业务请求使用 protobuf，上传和下载流式传输，不经过模型或整文件重复校验。支持目录递归复制；符号链接可进入目录，特殊文件和符号链接复制会明确报错；暂不提供目录打包下载。实测范围为当前 macOS 宿主与真实 Android/Linux 客体，不代表 Windows 宿主安装已验收。
