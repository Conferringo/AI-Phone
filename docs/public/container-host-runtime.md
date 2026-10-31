# Offline container host

AI Phone's container-host component supplies the installation's own container engine. The Apple Silicon profile uses Lima with Virtualization.framework; the Linux x64 profile uses Lima with QEMU, bundled SSH and firmware. Local ISO images and signed Docker/Compose packages support provisioning without repository access. Complete-client resources and platform integration are qualified separately in [release status](release-status.md).

## Factory inputs

`config/container-host-runtime.json` identifies each host's Alpine-Lima image, package closure and versions. `config/host-bootstrap.json` pins the architecture-matched Lima distribution. The producer checks hashes, inventory budgets, permissions and required files before publishing a component directory. A present but invalid component fails explicitly.

On Apple Silicon, `scripts/lima_native.py` builds Lima from pinned source using an isolated Go SDK and the client's declared deployment target. Its real Mach-O target is inspected; metadata cannot authorize a binary requiring a newer operating system.

On Linux x64, `scripts/linux_host_runtime.py` supplies QEMU, image tools, SSH, shared libraries and BIOS/UEFI firmware from a pinned factory archive. ELF admission and inventories share one implementation with [offline OCR](engineering/linux-offline-ocr.md). The host component includes this dependency inventory and selects its launchers through an installation-owned Lima wrapper. Libraries are selected for those processes without replacing system libraries.

## Maintainer build

These commands run on the matching factory host with reviewed inputs. They are not end-user setup steps.

For Apple Silicon, prepare the native Lima build first:

```sh
PYTHONPATH=scripts python3 scripts/lima_native.py \
  --source /factory/lima-source --go /factory/go/bin/go
```

For Linux x64, prepare the native dependency component first:

```sh
python3 scripts/linux_host_runtime.py \
  --archive /factory/linux-x64-host.tar.gz \
  --destination .tools/linux-host-runtime
```

Then construct the host component and include it in client resources:

```sh
python3 scripts/container_host_runtime.py \
  --lima /factory/lima.tar.gz --image /factory/alpine.iso \
  --packages /factory/apks

python3 scripts/build_client.py --platform linux \
  --with-container-host --with-web-runtime --with-service-runtime \
  --with-js-services
```

The last command illustrates component inclusion, not the complete distribution command. A complete client also needs matched Android, guest images/applications, installer tools and offline OCR. Factory source, cancellation and publication checks preserve previous outputs when a new build fails. Factory compilers and caches are not global user prerequisites.

## Runtime ownership

The VM mounts only its installation directory. Apple Silicon uses virtiofs; Linux QEMU uses 9p, the upstream [standard QEMU mount configuration](https://lima-vm.io/docs/config/mount/). Proxy environment, personal SSH public keys and SSH-agent forwarding are disabled. Provisioning uses local files with `apk --no-network`; the Docker wrapper carries only named resource/image settings into guest execution.

Persistent Lima state has a short path under `.aiphone-vms`, keyed by the resolved installation data directory. An ownership marker prevents accidental adoption. Missing, replaced or foreign state is reported. A private selection record restores the same wrapper, command and state after reopening; hashes and ownership are checked at startup. This does not add work to every model token or media frame. The engine does not change global Docker context or create a public endpoint.

A valid Linux host component also avoids installing another QEMU/SSH toolchain through apt. Graphics/audio libraries and KVM access prerequisites for the Android host remain separate packaging work; the complete Linux client is not yet qualified as fully offline.

## Qualification

Apple Silicon qualification includes actual VZ startup, offline signed-package installation, container execution with pulls disabled, and cold restart with retained data. The source-built Lima has a macOS 14.4 link target; physical execution on macOS 14.4 has not been tested. Reduced diagnostic disk/memory settings do not replace full default-size deployment acceptance.

The Linux x64 component has 611 inventoried files plus its manifest: 335,197,312 resource bytes in a 237,991,430-byte archive. A real Linux factory assembled it with networking disabled. The included Lima 2.2.1 executed and validated the production QEMU/9p definition. A separate reader unpacks and checks its hashes, permissions and host policy before executing Lima.

| Linux check | Evidence scope |
|---|---|
| QEMU, image tools and SSH | Executed without system QEMU/SSH or network; qcow2 creation/check and temporary key generation passed |
| Pinned Alpine image | TCG execution on Apple Silicon reached root login and returned `x86_64`; this is not KVM performance or installation acceptance |
| Signed package closure | 31 packages installed offline; Docker 29.9.2, Compose 5.6.1 and Buildx 0.38.1 executed |
| Serial version probe | A later combined version probe did not finish within 150 seconds; it is not recorded as passed |
| Docker daemon inside the packaged Lima VM | Not qualified by the CLI or raw QEMU checks |
| Complete Linux client | Matched remaining resources, compilation and payload inspection still required |

The first software-boot attempt exceeded a test container memory limit; Docker recorded OOM/exit 137. A subsequent appropriately sized test reached root and returned the architecture within the same deadline. Hardware-accelerated behavior remains distinct. Non-macOS physical installation is waived by the maintainer and is disclosed as untested, rather than marked passed. Binary publication and corresponding-source review remain separate gates. No user disks, accounts or chat history are factory inputs.

## 中文

容器宿主组件内置 Lima、本地 Alpine 镜像与签名 Docker/Compose 包。Apple Silicon 使用 VZ；Linux x64 使用随包 QEMU、SSH、动态库和 BIOS/UEFI 固件，通过安装目录内的启动器选择。Linux 的程序依赖与 OCR 共用归档、ELF 和清单校验；不替换用户系统库。

Linux 组件已实际装配，包含611个清单文件，归档约227 MiB；Lima 版本执行与 QEMU/9p 定义校验通过。断网环境已执行 QEMU/磁盘工具/SSH，官方镜像的软件模拟进入 root 并返回架构；31个包完成断网安装，Docker/Compose/Buildx执行通过。较严格的串口版本探针未在150秒内完成，没有记作通过。Lima 内 Docker 守护进程、完整 Linux 客户端及其余资源继续独立验收；非 macOS 实机安装按维护者要求豁免并标明未测。
