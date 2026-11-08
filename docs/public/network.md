# Device networking

Each guest has its own Clash application. Android uses Clash Meta; the Linux
desktop uses Clash Verge Rev. Manage subscriptions, nodes, VPN/TUN and startup inside
the device. The web configuration center manages devices and Agents; network
subscriptions stay in the guest application.

## Configure a device

- **Android:** open **Clash Meta**, import a subscription under Profiles, select
  the profile and start the VPN. Updates and automatic restart use the app's
  settings.
- **Linux:** open **Clash**, import a subscription and enable TUN. The app's
  startup preference controls whether it opens with the desktop.

New devices contain no developer subscription, nodes or accounts. Existing
application data and user-installed updates are preserved. Closing the app or
disabling its VPN/TUN remains under the user's control.

## Network boundaries

The host defaults to direct access. An Agent's explicit model proxy is independent
of guest networking. AI Phone does not change the host's system proxy or another
application's Clash instance.

Android uses its system VPN. The Linux app configures TUN as root inside the guest
container, with `NET_ADMIN`. It does not receive host networking, privileged Docker
access or the Docker socket; proxy and controller ports are not exposed to the
LAN. Guest root does not grant host execution privileges.

The Android root `curl` wrapper uses the device-local proxy while the VPN is active. Check routes from the affected application or process.

## Installation and startup

Complete factory resources include the Android APKs and the Linux app in the
desktop image. Source development setup can download the same pinned applications.
Versions, origins, sizes, SHA-256 digests and Android signer identities are recorded
in [`config/clash-apps.json`](../../config/clash-apps.json). See
[release status](release-status.md) for complete installer availability.

Linux recreates its temporary `/dev/net/tun` node on guest startup and opens Clash
according to the application's XDG startup setting. The application owns its routes
and core process. Android uses the native app's automatic restart preference.
Subscriptions remain private device data and are excluded from distribution inputs.

## Verification scope

The existing ARM64 development deployment, using its original Clash Party, has verified native app startup, separate
TUN routes, subscription import/refresh, HTTPS access, hashed downloads and recovery
after guest restart. An isolated empty Linux first boot was also checked. An Android
HTTPS event stream produced a system notification that the backend listener read.

New factory images pin Clash Verge Rev 2.4.2 and select its direct-core mode for
the lightweight container. Clash Verge Rev runtime validation is pending.

## Third-party software

[Clash Meta for Android](https://github.com/MetaCubeX/ClashMetaForAndroid/tree/v2.11.36)
and [Clash Verge Rev](https://github.com/clash-verge-rev/clash-verge-rev/tree/5dca724017278d441cad567da19bff9862a5b868) are
upstream GPLv3 projects. Distribution must include their applicable notices and
corresponding-source requirements.

## 中文

每台设备在系统内使用独立的 Clash 应用：Android 为 Clash Meta，新的 Linux 基线为 Clash
Verge Rev 2.4.2。打开设备桌面上的应用，导入订阅并启用 VPN/TUN；节点、更新和开机启动均由
应用自身管理。网页配置中心不保存设备订阅。新设备没有开发者节点、订阅或账号，
已有应用数据和用户更新保持。

宿主默认直联，Agent 显式配置的模型代理独立生效。Android 使用系统 VPN；Linux
仅在客体容器内使用 `NET_ADMIN` 配置 TUN，不开放宿主网络、Docker socket 或局域网
代理端口。客体 root 不获得宿主执行权限。Android root `curl` 包装器在 VPN 运行时使用设备本地代理。

完整工厂资源包含 Android APK 和 Linux 桌面应用，源码开发安装也可下载相同的固定
版本；来源、大小、摘要和 Android 签名见上方配置。用户订阅属于私有设备数据，不
作为发行输入。Linux 启动时重建 TUN 节点并遵循应用自启设置，Android 遵循原生
自动重启选项。完整安装包供应情况见[发行状态](release-status.md)。

现有 ARM64 开发环境使用原 Clash Party，已验证应用、独立 TUN、订阅导入/刷新、HTTPS、哈希下载、客体
重启恢复，以及 Android 事件流通知的后端接收。第三方客户端的 GPLv3 声明和对应源码义务
分别适用。

新镜像的 Clash Verge Rev 直接运行核心，运行验证待完成。
