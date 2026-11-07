# 原生客户端开发

[English](native-client.en.md) · 开发中

完整产品安装包尚未提供。发行包须包含系统镜像、模拟器与预构建服务，详见[发行标准](preview-release.md)。下述源码安装与连接实现仅供开发，不作为完整产品交付。

保存的服务可达时直接进入工作台。macOS 已完成且源码版本一致的安装可恢复所属服务，实际探测后进入；未完成安装提供重试。菜单「AI Phone → Setup」（Command/Control+,）返回安装或连接配置。失败不清空输入；停止可打断卡住的下载和安装进程组。进度、资源不足和错误在恢复时仍可见。

仅原生安装窗口拥有部署权限，远端工作台不能执行宿主安装命令。版本源码与用户数据分离，设备、账号、聊天、文件和模型密钥不随客户端覆盖。跨版本自动激活、升级和完整回滚仍未完成。

原生Rust Android运行进程可启动已准入的SDK/AVD，核对root与截图传输，并在安装握手管道关闭后保持手机运行；重复进入观察并复用同一实例。启动中取消先等待模拟器控制端口，再发一次停止命令；退出结果不明保留错误。旧归属记录存在但端口被占用时要求恢复，不重复启动。Windows安装已调用该进程，再初始化WSL；配对、客体配置与完整双设备就绪仍未完成。进程夹具与隔离macOS Android开机已分别验证，不代表Windows硬件加速或完整产品安装通过。

Rust客体手机peer通过私有protobuf标准输入接收配对凭据与模拟器令牌，管理现有媒体发现文件和本地桥监听；保留冲突文件，在连接收敛后移除发现信息。Linux ARM64进程已通过现有媒体客户端读到运行中的macOS Android root身份与真实截图。Windows自动配对、客体ADB/协议工具随包提供和完整客体配置仍须接通，完整宿主就绪标记保持关闭。

构建入口：`./scripts/bootstrap.sh`、`cargo test --manifest-path apps/client/src-tauri/Cargo.toml --locked`、`bun scripts/build-client.ts`。打包会重新生成确定性的源码白名单资源并核验摘要，其他平台使用 `--platform windows`。CI构建与解包检查不是实机安装验收。

macOS要求14.4以上及Apple Silicon；Windows需要WebView2。macOS/Windows未签名。详见[预览范围](preview-release.md)、[第三方声明](../../THIRD_PARTY_NOTICES.md)和[发行状态](release-status.md)。
