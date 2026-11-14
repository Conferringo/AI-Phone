# Distribution requirements / 发行标准

Ship one DMG for Apple Silicon macOS and one EXE for Windows 11 x64. Each installer contains clean Android and Linux baselines, the matching emulator, device control and media, and the prebuilt local services. The client handles first setup and device readiness.

Keep user state separate from application versions. Build installers from clean images and source. Include third-party notices, source provenance and applicable corresponding source. Review each Android SDK component under its own terms. [Android SDK terms](https://developer.android.com/studio/terms)

GitHub accepts release assets under 2 GiB. Users download a complete installer. [GitHub release limits](https://docs.github.com/en/repositories/releasing-projects-on-github/about-releases)

## 中文

Apple Silicon macOS 交付一个 DMG，Windows 11 x64 交付一个 EXE。每个包内置干净的 Android 和 Linux 镜像、匹配的模拟器、设备控制和媒体组件、预构建的本地服务。客户端负责配置与设备初始化。

用户数据与程序版本分开保存。使用干净镜像和源码构建，附上第三方声明、来源和适用的对应源码。GitHub 单附件小于 2 GiB，用户下载一个完整安装包。
