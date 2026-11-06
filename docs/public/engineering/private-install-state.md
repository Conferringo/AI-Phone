# Private installation state

Rust owns first-run profile validation, creation, explicit edits and initial database-password generation. The native writer is shared with WSL registration records. Legacy installer checkpoints and runtime selections still use the transitional Python writers. Platform operations remain separate from checkpoint and database policy:

| Owner | Responsibility |
|---|---|
| `crates/host-runtime/src/profile.rs` | Native profile validation/publication and unchanged-profile resume; never reconfigure existing devices on resume |
| `crates/host-runtime/src/filesystem.rs` | Native private temporary-file publication and directory sync for profiles and WSL ownership |
| `crates/assistant/src/database/bootstrap.rs` | Rust/SQLx role preparation during normal startup |
| `scripts/private_files.py` | Private temporary files, complete JSON write/flush, replacement and cleanup |
| `scripts/windows_private.py` | Current-user/SYSTEM DACLs, Win32 handle ownership and write-through publication |
| `scripts/installer_lock.py` | Nonblocking kernel lock and descriptor lifetime |
| `scripts/install_state.py` | Phase receipts, input/artifact matching and interrupted-work recovery |
| `scripts/database.py` | Remaining maintenance/restore orchestration and legacy credential validation |

The existing JSON formats and persistent data locations are unchanged. `database.atomic_json` remains available to existing callers through the shared implementation. The application does not rewrite existing credentials merely to start a newer version.

`--prepare-host-profile` consumes the existing bounded deployment request on stdin. Existing profiles retain their exact bytes, resource choices and optional legacy fields. Explicit edits use the same Rust validator through `--write-host-profile`; the contributor CLI no longer maintains its own schema/proxy/preset policy or writes profile/password files in Python. A failed new credential publication cannot leave a completed profile. Staged `data` and `.env` links retain their persistent user-directory owner.

The component inventory qualifies `ai-phone-host-profile-v1` before a packaged assistant can receive these commands. Source installations build the dedicated native helper once in their own target directory. No profile check or subprocess has been added to phone-use, computer-use or model-token paths. Host orchestration, checkpoint recovery and tool packaging are not fully migrated yet.

## Platform contract

On macOS/Linux, files use mode `0600`, private directories use `0700`, and publication flushes the temporary file, replaces the destination and flushes its parent directory. Installer contention uses nonblocking `flock`.

Windows uses a protected DACL granting full access only to the current process user and SYSTEM. This is a real access-control policy rather than interpreting POSIX mode bits as Windows ACLs. The adapter queries the current token's SID, sets/reads file security through [SetNamedSecurityInfoW](https://learn.microsoft.com/en-us/windows/win32/api/aclapi/nf-aclapi-setnamedsecurityinfow) and [GetNamedSecurityInfoW](https://learn.microsoft.com/en-us/windows/win32/api/aclapi/nf-aclapi-getnamedsecurityinfow), and releases allocated native descriptors/strings. Validation reads the native descriptor and ACEs directly: the DACL must be protected and present, with exactly two applicable allow entries granting full file access to the current user and SYSTEM. SID identities are compared through the Windows API, independently of descriptor spelling or entry order. Additional principals, incomplete rights, inherit-only entries and malformed SID lengths are rejected.

Windows publication flushes file data before a same-directory [MoveFileExW](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexw) replacement with write-through. Contention uses [msvcrt's nonblocking byte-range lock](https://docs.python.org/3/library/msvcrt.html), always at offset zero. Closing the descriptor or terminating its owner releases the lock. There is no PID-file-only claim of exclusivity and no retry sleep in the lock operation.

A symbolic-link substitution is preserved and rejected. An incomplete write cannot replace the previous JSON file. A publication error must be resolved from the resulting state before work is retried; a failed return does not justify discarding credentials or recreating devices.

## Availability

Current platform support, installer availability and distribution requirements are maintained in [release status](../release-status.md). Component formats and build interfaces do not independently qualify a complete installation.

## 中文

首次配置、显式编辑、校验和初始数据库密码生成已由Rust承担；原生写入器与WSL归属记录共用。重开保留原配置字节、设备资源与密码，显式编辑不再重复维护Python校验/写入实现。源代码与持久用户目录的data/.env链接保持；新凭据未发布成功不会留下已完成配置。正常角色准备使用Rust/SQLx，其余检查点、工具与恢复编排仍有待退役的Python实现。

遗留检查点层在macOS/Linux保持私有模式与目录同步；Windows使用当前用户和SYSTEM的受保护DACL、非阻塞字节锁及写穿替换。原JSON格式、数据位置及凭据保持，失败不触发重建设备或清空用户数据。

文件权限与锁保证私有状态的发布边界；完整虚拟化、设备创建、媒体与升级另有宿主集成要求。当前范围见[发行状态](../release-status.md)。
