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

## Availability

The component supplies its execution environment; Android acceleration, full device creation, graphics/audio and update integration remain separate host requirements. Current platform support and installer availability are maintained in [release status](release-status.md). Third-party notices and corresponding-source requirements apply to binary distribution.

## 中文

容器宿主组件内置 Lima、本地 Alpine 镜像与签名 Docker/Compose 包。Apple Silicon 使用 VZ；Linux x64 使用随包 QEMU、SSH、动态库和 BIOS/UEFI 固件，通过安装目录内的启动器选择。Linux 的程序依赖与 OCR 共用归档、ELF 和清单校验；不替换用户系统库。

容器宿主、Android 硬件加速、双设备创建及媒体集成分别由对应组件负责。当前平台范围、安装包供应与分发条件统一见[发行状态](release-status.md)。
