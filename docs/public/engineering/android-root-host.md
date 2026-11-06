# Rooted Android host worker

This Rust execution unit prepares an application-owned Linux runtime and runs one foreground ARM64 Linux guest on rooted Android. It is a building block for the standalone Android host, **not a complete APK or dual-device implementation**. The client capability remains disabled until matching resources and the complete setup path exist.

## Native contract

`host-resources --android-root-contract` returns `ai-phone-android-root-v1`. The installer invokes the following private commands as Android root:

```sh
host-resources --android-root install < install.frame > prepared.frame
host-resources --android-root run < start.frame > exited.frame
```

Frames use the existing four-byte big-endian length prefix followed by protobuf. Messages are `NativeAndroidRootInstall`, `NativeAndroidRootPrepared`, `NativeAndroidRootStart` and `NativeAndroidRootExited` in `native_android.proto`. Guest serial output goes to stderr, separate from the protobuf reply. These commands are installer IPC, not Agent tools or public network endpoints.

The installer admits the bundled archive before invoking extraction. The worker stages it under the existing installation lock, restores the private runtime-root mode and publishes `rootfs` only after extraction and required-file checks succeed. An existing runtime is preserved and reported as already installed; this primitive does not resize, replace or upgrade a user's devices. Cancellation discards unpublished temporary files.

Installation synchronizes copied files once before publication, then synchronizes the parent directory after the rename. This is an installation commit boundary, not a per-action or per-frame check. It addresses a reproduced Android restart in which unsynchronized extracted files retained their names but became empty.

The prepared root contains `/usr/bin/qemu-system-aarch64`, `/opt/ai-phone/guest/kernel`, `/opt/ai-phone/guest/initramfs.gz`, and its Linux runtime dependencies. The kernel/initramfs interface currently runs a serial Linux guest without networking or a persistent desktop disk. The future full image and device factory must provide those capabilities separately.

## Process and mount ownership

The worker requires Android root and checks the Android SDK property. KVM is the default accelerator; failure to open `/dev/kvm` does not select TCG. `diagnostic_tcg` is an explicit maintainer request for kernel/bootstrap testing and does not qualify the supported root/KVM host target.

One worker holds the guest lease, bind-mounts that Android instance's `/dev` and `/proc` into its owned root, and starts QEMU in its own process group. SIGINT/SIGTERM use the existing service termination contract. The worker waits for the child and unmounts its own bind mounts before returning. A cleanup failure remains an error; it is not reported as a successful shutdown. Kernel/initramfs output and process exit do not certify desktop, phone, media or application readiness.

## Remaining integration

Isolated Android 35 ARM64 verification covers real protobuf installation, an immediate stop/restart with byte-identical kernel/initramfs, a Linux guest boot and shutdown, cancellation of a running QEMU process with bind-mount cleanup, and cancellation/resumption of extraction while preserving existing test data. SELinux stays Enforcing. The test instance lacks `/dev/kvm`, so these guest boots use explicitly requested TCG and do not qualify KVM performance or hardware support.

The APK still needs bundled and reviewed ARM64 host resources, privileged worker launch, supervised persistence, a prepared GUI Linux system, an Android guest, media/input/notification transport, first-run configuration and real root/KVM qualification. Source compilation and a tiny diagnostic guest cannot replace those requirements. See [release status](../release-status.md).

## 中文

这是供已root Android使用的Rust底层执行单元，不是完整独立APK或双设备交付。安装与运行复用原生protobuf管道；客体串口写入stderr，不混入结果帧。解包只发布完整的新运行目录，已有运行环境和用户数据保留。

默认使用KVM，不因失败回退到TCG；TCG仅供显式诊断。执行器管理QEMU进程组、取消及自身挂载点的释放，进程退出回执不等于桌面、手机或应用已就绪。目前仍需补齐APK资源与首次配置、持久GUI系统、Android客体、媒体和通知，以及真实root/KVM验收，客户端支持标志保持关闭。
