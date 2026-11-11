# Maintainer delivery volumes

Large installers retain their complete DMG or EXE format. Delivery
volumes divide those bytes into files of at most 1 GiB. Each volume has a size
and SHA-256 digest; the manifest also pins the original installer size and digest.
The release owner supplies the manifest digest separately.

The Rust `package-volumes` program builds, verifies and restores this format.
Verification streams every volume without creating another installer-sized copy.
Restoration writes a temporary file, checks each volume and the complete installer,
then activates the destination without replacing an existing file. Cancellation,
invalid input or insufficient storage does not publish a partial installer.

These volumes are for maintainer transport. Public releases ship one complete installer under the single-asset limit.

```sh
cargo run --locked -p ai-phone-host-runtime --bin package-volumes -- \
  split installer.exe installer.exe.volumes INSTALLER_SHA256
cargo run --locked -p ai-phone-host-runtime --bin package-volumes -- \
  verify installer.exe.volumes/installer.exe.volumes.json MANIFEST_SHA256
cargo run --locked -p ai-phone-host-runtime --bin package-volumes -- \
  restore installer.exe.volumes/installer.exe.volumes.json downloads MANIFEST_SHA256
```

## Candidate retention

Private Actions storage retains the installer, manifest and checksum. The maintainer checks the receipt and redistribution terms before uploading the complete installer to Releases.

## 中文

维护交付卷按 1 GiB 分段，以流式 SHA-256 验证后原子还原。用户下载单个完整安装包。
