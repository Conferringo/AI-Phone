# 原生客户端开发

[English](native-client.en.md)

Tauri 客户端负责本机配置。两端都运行原生 Android。macOS 使用 Lima 运行 Linux 环境，Windows 使用 QEMU 和 Windows Hypervisor Platform 启动内置 Linux 客体。初始化完成后，工作台连接本地服务。

配置界面支持十种语言、键盘操作、重试和取消。通过 **AI Phone → 设置** 返回配置。

## 构建

```sh
./scripts/bootstrap.sh
./scripts/bun scripts/build-client.ts --with-web-runtime
```

这条命令构建开发客户端。完整安装包见 [macOS 工厂](engineering/macos-full-runtime.md)及 [Windows 工厂](windows-client-factory.md)。

macOS 需要 Apple Silicon 和 macOS 14.4 或更高版本。Windows 安装包适用于 Windows 11 x64。[下载与状态](release-status.md)
