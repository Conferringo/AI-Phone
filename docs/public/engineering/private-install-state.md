# Private installation state

Installation checkpoints, credentials and runtime selections use one atomic JSON writer. Platform operations are separate from checkpoint and database policy:

| Owner | Responsibility |
|---|---|
| `scripts/private_files.py` | Private temporary files, complete JSON write/flush, replacement and cleanup |
| `scripts/windows_private.py` | Current-user/SYSTEM DACLs, Win32 handle ownership and write-through publication |
| `scripts/installer_lock.py` | Nonblocking kernel lock and descriptor lifetime |
| `scripts/install_state.py` | Phase receipts, input/artifact matching and interrupted-work recovery |
| `scripts/database.py` | Database identity, role preparation and credential validation |

The existing JSON formats and persistent data locations are unchanged. `database.atomic_json` remains available to existing callers through the shared implementation. The application does not rewrite existing credentials merely to start a newer version.

## Platform contract

On macOS/Linux, files use mode `0600`, private directories use `0700`, and publication flushes the temporary file, replaces the destination and flushes its parent directory. Installer contention uses nonblocking `flock`.

Windows uses a protected DACL granting full access only to the current process user and SYSTEM. This is a real access-control policy rather than interpreting POSIX mode bits as Windows ACLs. The adapter queries the current token's SID, sets/reads file security through [SetNamedSecurityInfoW](https://learn.microsoft.com/en-us/windows/win32/api/aclapi/nf-aclapi-setnamedsecurityinfow) and [GetNamedSecurityInfoW](https://learn.microsoft.com/en-us/windows/win32/api/aclapi/nf-aclapi-getnamedsecurityinfow), and releases allocated native descriptors/strings. Windows normalizes expected and actual SID spellings through its descriptor formatter, including built-in-account aliases. Additional access grants are rejected by private-file validation. A descriptive auto-inheritance flag does not change the required protected DACL or its exact access entries.

Windows publication flushes file data before a same-directory [MoveFileExW](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexw) replacement with write-through. Contention uses [msvcrt's nonblocking byte-range lock](https://docs.python.org/3/library/msvcrt.html), always at offset zero. Closing the descriptor or terminating its owner releases the lock. There is no PID-file-only claim of exclusivity and no retry sleep in the lock operation.

A symbolic-link substitution is preserved and rejected. An incomplete write cannot replace the previous JSON file. A publication error must be resolved from the resulting state before work is retried; a failed return does not justify discarding credentials or recreating devices.

## Evidence and remaining acceptance

macOS tests exercise actual process contention, release after forced process termination, checkpoint reuse, private modes, interrupted writes and temporary-file cleanup. A labelled disposable PostgreSQL instance passed the role/backup/restore regressions with the shared writer; production data was not a fixture.

Windows-specific tests exercise real ACL creation, rejection of an added world-read grant, native error capture and process locks when run on a Windows worker. Cross-platform mock tests cover native buffer limits, errors and handle cleanup. A macOS mock run does not establish Windows native behavior; inspect the matching CI run before recording that acceptance.

These primitives are part of Windows host preparation. They do not establish complete Windows virtualization, guest creation, ordinary-user dependency aliases, long-path handling, audio or upgrade acceptance. Complete installers remain subject to the [distribution gates](../release-status.md).

## 中文

检查点、凭据和运行选择共用一个私有原子 JSON 写入层；检查点与数据库策略不再直接承担操作系统文件锁与权限接口。macOS/Linux 保持私有模式与目录同步；Windows 使用当前用户和 SYSTEM 的受保护 DACL、非阻塞字节锁及写穿替换。原 JSON 格式、数据位置及凭据保持，失败不触发重建设备或清空用户数据。

本机已实测进程互斥、强制终止后的锁释放、写入失败保留旧文件，以及隔离 PostgreSQL 的权限和备份恢复。Windows 真实 ACL/API 与进程锁必须以匹配 Windows CI 的结果为准；这不代表 Windows 双设备完整宿主已经交付。
