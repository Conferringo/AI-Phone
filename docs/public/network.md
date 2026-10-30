# Device networks / 设备独立网络

2026-10-28。网络由每台设备内部的 Clash 图形应用管理。网页配置中心没有订阅或节点设置。

## 使用方式

- Android：桌面打开 **Clash Meta**，在「配置」导入自己的订阅，选择配置并启动 VPN；在应用内管理节点、更新和自动重启。
- Linux：桌面打开 **Clash**（Clash Party），导入自己的订阅并启用 TUN。应用设置中可选择开机启动；关闭该选项后，AI Phone 不强制恢复代理。
- 新设备预装客户端，默认没有订阅、节点或开发者账号，用户自行决定是否启动。已有设备的私有配置留在原设备中。

Android 使用系统 VPN，Linux 使用客体网络命名空间内的 TUN。桌面客户端在客体内以 root 运行以配置路由；不获得 macOS 宿主机执行权限。Linux 仅增加 NET_ADMIN，不使用 host network、privileged 或 Docker socket，不映射代理/控制端口到局域网。

项目宿主默认直联，Android 不继承宿主 Clash。Agent 的模型代理是独立、显式的配置；已有模型代理不会被网络迁移清空。宿主自身的系统代理由用户管理，本项目不关闭其他项目或宿主 Clash。

## 生命周期

安装器从官方发行下载固定版本的 Android APK 和 Linux deb，校验大小和 SHA256；版本、来源与 Android 签名指纹见 `config/clash-apps.json`。Linux 镜像内安装原版客户端，Android 创建流程幂等安装，已有应用数据和用户更新版本保留。

Linux 每次启动创建临时 `/dev/net/tun` 节点；桌面启动后根据原生应用的 XDG 自启选项打开客户端。代理路由由客户端自身管理，不由网页反复拉起。Android 使用客户端的原生自动重启选项。关闭应用/VPN 是用户可控操作。

`apps/device-network/control.ts` 和 `infra/network` 保留旧部署的迁移与诊断能力。标记为应用管理后，旧后台核心不会自动恢复，旧网页网络 API 已移除；不会用退役控制器误报原生客户端失败。旧配置仅作为私有备份保留。root curl 包装器在原生 Android VPN 运行时连接设备本地代理，不能据此承诺任意 root 程序都会自动经过 VPN。

## 本机真实验收

- Android Clash Meta 2.11.36 和 Linux Clash Party 2.0.4 的真实 GUI、独立 TUN 与国内/境外 HTTPS、固定哈希文件下载。
- 手机客户端直接刷新原订阅成功，Linux 原生客户端导入订阅成功；未把缓存预置当作刷新成功。
- 手机系统与 Linux 容器分别重启后，两端 GUI 核心/TUN 恢复，六项 HTTPS/下载检查再次通过。Linux 重启时 TUN 节点丢失的问题已定位并修复。
- 空白隔离 Linux 容器实际启动，Clash 程序/桌面入口/TUN 节点齐全，没有开发者订阅或账号；Android 重复安装流程保留现有应用数据。
- 普通 Android 应用通过外部 HTTPS 事件流收到随机测试消息并生成系统通知，后端监听与只读 Agent 摘要有真实验证；不向真实社交联系人发送测试消息。

上述不是所有地区/订阅、微信实号推送、FCM、UDP、宿主整机重启/休眠或长期稳定性的认证。节点 DNS、分流和订阅更新由原生客户端及用户选择的服务决定。不能通过修改设备型号解除平台账号限制。

## English

Clash is an application **inside each guest**, not a network page in the web configuration center. Android ships Clash Meta; Linux ships Clash Party. Import your own subscription and control VPN/TUN, nodes, updates and startup in the native app. Fresh devices contain no developer subscription or account.

The host defaults to direct access. Explicit Agent model proxies are separate. Guests never inherit a new host proxy automatically. Linux uses NET_ADMIN within its isolated container, with no host networking or exposed proxy/controller ports. The app's startup preference is respected; the retired background core is not restarted after handoff.

Official app versions, sizes, hashes and sources are pinned in `config/clash-apps.json`. Source archives exclude APK/deb files, subscriptions and device disks. The installer downloads verified binaries separately. A network that cannot reach GitHub may set `AI_PHONE_BOOTSTRAP_PROXY` for installation downloads only.

Actual guest GUI/TUN, native subscription import/refresh, HTTPS, hashed downloads and guest restart recovery passed on the current ARM64 deployment. An empty isolated Linux first boot was also checked. This does not certify a fresh host installation, real WeChat push, every route/protocol or long-duration recovery.

Upstream sources and GPLv3 licences: [Clash Meta for Android](https://github.com/MetaCubeX/ClashMetaForAndroid/tree/v2.11.36), [Clash Party](https://github.com/mihomo-party-org/clash-party/tree/v2.0.4). Binary/image redistribution still requires the applicable notices and corresponding-source obligations; a matching hash is not a completed licensing review.
