# Offline container host / 离线容器宿主

This is the macOS ARM64 engine component, not a complete downloadable client. It removes the need for first-run downloads of Lima, a container-host image and Docker packages. Other platforms need their own matched host adapters.

## Inputs and build

`config/container-host-runtime.json` pins the clean Alpine-Lima ISO and the complete signed APK closure for Docker/Compose. The Lima archive pin stays in `config/host-bootstrap.json`. `scripts/lima_native.py` builds the pinned Lima source using an isolated Go SDK/cache and the client's declared macOS deployment target; the binary is signed and its real Mach-O target checked. A vendor executable requiring a newer OS cannot be admitted by changing metadata.

Maintainer build steps use already acquired, digest-checked inputs. These commands are **not user installation steps**:

```sh
PYTHONPATH=scripts python3 scripts/lima_native.py \
  --source /private/factory/lima-source \
  --go /private/factory/go/bin/go
PYTHONPATH=scripts python3 scripts/container_host_runtime.py \
  --lima /private/factory/lima.tar.gz \
  --image /private/factory/alpine.iso \
  --packages /private/factory/apks
```

The producer checks source commit/cleanliness, compiler version, binary version, checksums, actual minimum OS, archive budgets and package completeness. It publishes `.tools/container-host` only after the complete inventory passes. Compilation cancellation stops its owned process group; outputs from failed builds are not valid components. Go and factory caches are maintainer dependencies and are not installed globally on user computers.

`prepare_client_host.py --with-container-host` adds the component to the verified source payload; `build_client.py --with-container-host` is currently macOS-only. The component requires the other complete-client resources too; this flag alone does not produce a finished release.

## Runtime

- The staged component contains the Lima runtime, local ISO and signed packages. Provisioning uses `apk --no-network` with local files. It neither runs apt nor acquires packages from a repository.
- The VM mounts only the installation directory. It does not inherit proxy environment, personal SSH public keys or an SSH agent. The app-owned Docker wrapper carries only named resource/image settings into guest execution.
- Persistent Lima state uses an actual short path under the user's `.aiphone-vms`, keyed by the resolved installation data directory. A symlink shortcut is insufficient because Lima canonicalizes socket paths. An ownership marker prevents accidental adoption; missing, replaced or foreign state is reported rather than recreated during environment reads.
- A private selection record restores the same wrapper, command and state after reopening. Their hashes and ownership are checked at runtime startup, not for every model token or media frame. Active services are not force-restarted. Unknown wrapper files are preserved.
- Port forwarding preserves the existing service boundary; the engine does not open a public endpoint or change global Docker context. Complete bundles select this engine before probing any global Docker installation.

## Verification and limits

Actual macOS ARM64 VZ startup, offline signed-package installation and `--pull never` container execution have been exercised in a disposable VM. A cold restart preserved a container and its file. The source-built Lima has a real macOS 14.4 link target; execution on physical macOS 14.4 has not been tested.

Diagnostic tests use 1 GiB VM memory and an 8 GiB disk to test the engine lifecycle. These are not production presets or a waiver of the 35 GiB fresh-install gate. They do not prove full backend/desktop/phone initialization. Complete-client media/channel/browser integration, cross-platform engines, long-running recovery and full binary redistribution/corresponding-source review remain open. No production device disks, accounts or chat history enter the component.

中文：本组件把容器宿主所需的Lima、干净ISO及Docker/Compose依赖放入包内。维护者构建时固定源码与编译器，真实链接目标由客户端配置决定；用户安装时不下载包、不使用全局Go/Docker，也不接管已有Docker。安装私有目录挂载、短路径持久状态和环境恢复各自有归属，异常不自动收编。真实诊断VM验证不替代完整双设备初装或发行许可审阅。
