# 原生客户端开发

[English](native-client.en.md) · 开发中

完整产品安装包尚未提供。发行包须包含系统镜像、模拟器与预构建服务，详见[发行标准](preview-release.md)。下述源码安装与连接实现仅供开发，不作为完整产品交付。

保存的服务可达时直接进入工作台。macOS 已完成且源码版本一致的安装可恢复所属服务，实际探测后进入；未完成安装提供重试。菜单「AI Phone → Setup」（Command/Control+,）返回安装或连接配置。失败不清空输入；停止可打断卡住的下载和安装进程组。进度、资源不足和错误在恢复时仍可见。

仅原生安装窗口拥有部署权限，远端工作台不能执行宿主安装命令。版本源码与用户数据分离，设备、账号、聊天、文件和模型密钥不随客户端覆盖。跨版本自动激活、升级和完整回滚仍未完成。

构建入口：`./scripts/bootstrap.sh`、`cargo test --manifest-path apps/client/src-tauri/Cargo.toml --locked`、`python3 scripts/build_client.py`。打包会重新生成确定性的源码白名单资源并核验摘要，其他平台使用 `--platform windows|linux|android`。CI构建与解包检查不是实机安装验收。

macOS要求14.4以上及Apple Silicon；Linux包针对Ubuntu24.04兼容环境；Windows需要WebView2；Android为ARM64。macOS/Windows未签名，Android使用保留的预览签名身份。详见[预览范围](preview-release.md)、[第三方声明](../../THIRD_PARTY_NOTICES.md)和[发行状态](release-status.md)。
