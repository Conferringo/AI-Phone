# Native dependency backports

The native client still depends on the GTK3 generation used by Tauri/Wry. Updating only GLib to a different major API generation is not a compatible fix.

`apps/client/src-tauri/vendor/glib` contains the complete `glib 0.18.5` crate, with its original MIT licence. The only code change is the exact two-line upstream fix for [RUSTSEC-2024-0429](https://rustsec.org/advisories/RUSTSEC-2024-0429.html): the C output pointer is declared mutable and passed as `&mut p`. [Upstream commit](https://github.com/gtk-rs/gtk-rs-core/commit/b5a4071e439bef2b5eea76c3aa25e5ae84839e34).

[The manifest](../../config/native-backports.json) records the original crate archive hash, original and patched source hashes, upstream commit and every vendored file hash. The original archive's installation marker `.cargo-ok` is omitted. Cargo selects this source using a local patch; the original dependency version is retained instead of pretending an upstream patched release exists.

Source checks and dependency audits reject any changed, missing or added vendor file, symlink or disconnected Cargo patch. The advisory remains in the report with an explicit verified-backport disposition. Other advisories are not ignored. Native CI runs the optimized string-iterator regressions on Linux. A verified source patch is separate from platform installation and runtime acceptance; consult the platform report for actual runs.

原生客户端保留原始 MIT 许可证，采用上游精确两行修复，不跳过整个 GLib 公告。逐文件哈希、Cargo 路径和包身份均纳入检查；修复验证与平台运行验收分别记录。
