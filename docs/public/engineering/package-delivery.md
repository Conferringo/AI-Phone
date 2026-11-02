# Installer delivery volumes

Large installers retain their complete DMG or Debian package format. Delivery
volumes divide those bytes into files of at most 1 GiB. Each volume has a size
and SHA-256 digest; the manifest also pins the original installer size and digest.
The release owner supplies the manifest digest separately.

The Rust `package-volumes` program builds, verifies and restores this format.
Verification streams every volume without creating another installer-sized copy.
Restoration writes a temporary file, checks each volume and the complete installer,
then activates the destination without replacing an existing file. Cancellation,
invalid input or insufficient storage does not publish a partial installer.

The macOS and Linux complete-package factories use this handoff for installers
that exceed the distribution service's single-asset limit. Component inventories,
third-party terms and publication gates apply to the complete installer and all
of its volumes. Splitting does not remove system images or runtime dependencies.

```sh
cargo run --locked -p ai-phone-host-runtime --bin package-volumes -- \
  split installer.deb installer.deb.volumes INSTALLER_SHA256
cargo run --locked -p ai-phone-host-runtime --bin package-volumes -- \
  verify installer.deb.volumes/installer.deb.volumes.json MANIFEST_SHA256
cargo run --locked -p ai-phone-host-runtime --bin package-volumes -- \
  restore installer.deb.volumes/installer.deb.volumes.json downloads MANIFEST_SHA256
```

These are maintainer commands. Automatic retrieval and restoration in the graphical
installer remain a separate integration requirement; command-line reconstruction
does not qualify the one-click installation experience.

## 中文

完整安装包超过单附件上限时，交付卷将原始 DMG 或 Debian 安装包按最多 1 GiB 分段，
逐卷及完整文件均使用 SHA-256 校验，清单摘要由发行方单独固定。Rust 程序负责分卷、
无额外全包副本的流式验证和原子还原；失败、取消或空间不足不发布半成品，也不覆盖
已有安装包。图形安装器的自动获取与还原仍需独立接通，维护命令不代表一键安装验收。
