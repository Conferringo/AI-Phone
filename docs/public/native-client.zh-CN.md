# 原生客户端预览

[English](native-client.en.md) · 2026-10-28 · 四平台客户端，本地测试包。各平台构建与运行状态见下表。

当前有真实Tauri 2客户端、`.app`和DMG。启动后直接打开已配置的本机工作台，不显示欢迎/森林/进入页；本地所有者会话自动恢复，原账号和聊天保留。设备、模型及API在已有唯一配置中心中配置。客户端关闭不停止后台Agent或设备服务。

本包连接**已经部署并启动的服务器**，不包含完整服务器、Android SDK、设备系统盘、Docker、用户资料或密钥。全新机器免编译部署、受管理运行时下载、安装/升级编排仍未完成，不能把当前客户端包写成完整的一键部署产品。现已有 Windows x64 NSIS、Linux ARM64 DEB 和 Android ARM64 APK；具体运行验收范围见[多平台验收](multiplatform-clients.md)。

## 使用

1. 先完成[源码部署](getting-started.zh-CN.md)并配置设备/Agent。
2. 打开DMG，将`AI Phone.app`拖入「应用程序」，启动应用。已有本机部署默认使用`http://127.0.0.1:17860/phone`。
3. 需要连接自己的局域网服务器时，在原生菜单「AI Phone → Connection…」（Command+,）填写服务器根地址。只有连接成功才保存；失败可直接修改重试。当前仅允许本机/私有IP，拒绝嵌入账号、密码、查询串及页面路径。

连接设置只保存服务器地址，不保存模型API、账号或聊天。macOS保存于`~/Library/Application Support/dev.aiphone.client/server.json`。工作台页面没有原生任意执行能力；原生IPC仅本地连接窗口可读取/保存地址，另有窗口身份及来源校验。HTTP不等于加密连接，跨不可信网络的配对/TLS仍是发行门槛。

`./ai-phone open`优先打开匹配本项目Bundle ID的已构建/已安装客户端，找不到时打开浏览器工作台。该命令不会把用户保存的客户端服务器地址重置为开发者地址。

## 构建与校验

```sh
./scripts/bootstrap.sh
cargo test --manifest-path apps/client/src-tauri/Cargo.toml --locked
python3 scripts/build_client.py
```

构建使用锁定的Tauri CLI/Rust依赖，输出`dist/AI-Phone-<version>-macos-arm64.dmg`、SHA256及清单。客户端拥有独立Cargo工作区，避免把Linux桌面WebKit构建依赖混入现有服务器CI；现有独立macOS原生CI会执行测试、Clippy、依赖公告检查及未签名打包；工作流仅准备，尚未上传/远端运行。Windows/Linux/Android构建入口及CI矩阵已补齐，远端CI尚未执行。

本地预览未做Developer ID签名或公证，只有Mach-O的链接器ad-hoc签名；这不是已通过商店/全球分发验收的安装包。客户端原生连接界面支持中英，工作台复用八语目录，完整语言和WebKit媒体/输入/附件验收仍单独记录。

包内包含项目许可、隐私/免责声明和锁定客户端依赖的许可文本集合。部分上游crate发布包没有附带完整许可文本，清单中保留缺口；二进制/镜像的最终分发审查仍未通过。参见[第三方声明](../../THIRD_PARTY_NOTICES.md)和[发行状态](release-status.md)。不上传源码、安装包或私有状态。

打包前逐个检查macOS包文件：标识/版本/最低系统版本、许可资源一致性、异常私有文件、开发者路径和凭据模式，并生成DMG及包内文件哈希。参见[客户端发行审计](client-release-audit-2026-10-28.md)；校验和不等于签名或公证。


## 其他平台

| 平台 | 本地制品 | 已验证范围 |
| --- | --- | --- |
| Windows x64 | NSIS `.exe` | MSVC跨编译、解包、PE架构与许可/私有信息扫描；Windows真机运行待验证 |
| Linux ARM64 | `.deb` | Debian 13安装、非root原生窗口、连接错误反馈、已配置服务器自动启动及两台设备画面；Ubuntu/Fedora尚未实测 |
| Android ARM64 | `.apk` | Android 15安装、发布构建启动、地址编辑、连接局域网工作台、原生返回；物理手机与其他API版本待验证 |

Android 首次打开填写服务器的**已配置局域网地址**；之后直接连接。系统返回键/手势返回连接页，再返回会退到后台。不要填写 Android 自己的 localhost；模拟器的 NAT 别名也不等于服务已允许的访问地址。桌面客户端使用原生「AI Phone → Connection…」切换服务器。

```sh
python3 scripts/build_client.py --platform windows
python3 scripts/build_client.py --platform linux
python3 scripts/build_client.py --platform android
```

Linux 在 Linux 宿主构建（DEB最低glibc 2.39、GTK3.24、WebKitGTK2.40）；Windows 可在装有MSVC Build Tools的Windows构建，或使用cargo-xwin + LLVM lld-link + NSIS跨编译。跨编译的`XWIN_CACHE_DIR`不能含空格。展开NSIS的检查需要7-Zip。

Android 使用JDK17、SDK/build-tools36、NDK27.0.12077973、Rust aarch64-linux-android目标，最低Android9/API28。构建脚本会从Tauri模板生成忽略目录，再应用版本化overlay；不要手改生成目录。发布构建保留R8优化并载入Android默认反射/JNI规则，避免Jackson枚举被裁剪导致启动崩溃。品牌图标和单Activity返回行为也由overlay生成。

所有包只是客户端。Windows无Authenticode签名，macOS无Developer ID/公证，Android使用本机预览证书（不是正式发行身份）；私钥只存忽略的`data/client-signing`，不进入源码/安装包。正式更新需要保持同一签名身份，不能把本地预览证书作为公开发行密钥。每个制品都有SHA256和展开文件清单；校验和不代表发行签名。
