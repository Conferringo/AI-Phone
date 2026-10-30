# Distribution requirements / 发行标准

No product binaries are currently published. A native window that only connects to a separately deployed service is a development client, not the required distribution.

The next per-platform package must include:

- A clean Android baseline, matching emulator and ADB/control components.
- A Linux GUI system with its required runtime, file/media/input integration and preinstalled device networking application.
- Prebuilt Agent, web, database/coordination and host supervision components. End users do not compile source or install an SDK.
- Versioned file hashes, architecture/resource metadata, source provenance, full third-party notices and required corresponding source.
- Client-owned first setup, persistent user-data separation, interruption recovery and an actual two-device ready check.

Never build a distribution from a live user's guest disk or database. Model keys, accounts, chats, contacts, subscriptions, attachments and private verification evidence are excluded. Existing installations are not reset when code or base images change.

Google SDK download archives are not automatically redistribution-cleared. SDK terms distinguish the SDK grant from components governed by open-source licences; each shipped image/emulator/component needs its own reviewed source and terms. The project may need source-built AOSP/emulator artifacts. Original Apache-2.0 code does not relicense third-party binaries.

GitHub requires each release asset to be under 2 GiB. If the full package exceeds that limit, use verified volumes with automatic client assembly or a suitable full-package mirror; users must not manually find device dependencies. The complete payload must exist and be checked before publishing any installer.

Sources: [Android SDK terms, §§3.4–3.5](https://developer.android.com/studio/terms), [GitHub release limits](https://docs.github.com/en/repositories/releasing-projects-on-github/about-releases).

---

# 中文标准

完整发行包必须包含干净的 Android/Linux 系统、匹配模拟器、预构建服务和必要运行组件。用户只在客户端完成资源与模型配置，设备初始化自动完成，不另找镜像、不安装 SDK、不编译服务、不手动部署后端。

禁止把当前用户的设备盘、数据库、账号、聊天、联系人、订阅或密钥打进镜像。版本代码、基础镜像与用户数据分离；更新不能覆盖现有用户内容。

镜像、模拟器和所有第三方组件需要逐项来源与再分发依据，必要时从 AOSP/模拟器源码构建，并提供对应源码与声明。不能把项目的 Apache-2.0 许可证当成第三方二进制授权。超过 GitHub 单附件限制时，由客户端处理已校验的完整包分卷，不把依赖准备推给用户。
