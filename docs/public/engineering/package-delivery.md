# Maintainer delivery volumes

Large installers retain their complete DMG or Debian package format. Delivery
volumes divide those bytes into files of at most 1 GiB. Each volume has a size
and SHA-256 digest; the manifest also pins the original installer size and digest.
The release owner supplies the manifest digest separately.

The Rust `package-volumes` program builds, verifies and restores this format.
Verification streams every volume without creating another installer-sized copy.
Restoration writes a temporary file, checks each volume and the complete installer,
then activates the destination without replacing an existing file. Cancellation,
invalid input or insufficient storage does not publish a partial installer.

This format serves maintainer transport and existing internal candidates. User
distributions require one complete installer file. The macOS, Windows and Linux
complete factories reject installers at or above the single-asset limit; splitting
does not qualify them for release. Component inventories, third-party terms and
publication gates still apply to internal volumes and their complete contents.

```sh
cargo run --locked -p ai-phone-host-runtime --bin package-volumes -- \
  split installer.deb installer.deb.volumes INSTALLER_SHA256
cargo run --locked -p ai-phone-host-runtime --bin package-volumes -- \
  verify installer.deb.volumes/installer.deb.volumes.json MANIFEST_SHA256
cargo run --locked -p ai-phone-host-runtime --bin package-volumes -- \
  restore installer.deb.volumes/installer.deb.volumes.json downloads MANIFEST_SHA256
```

These are maintainer commands. Users are not asked to retrieve or reconstruct
volumes. Command-line reconstruction does not qualify one-click installation.

## Candidate retention

The complete factories can explicitly retain an installer in an unpublished GitHub
draft for maintainer review. The retention plan checks the source identity,
installer metadata, complete resource status and exact upload inventory. It
excludes factory logs and unrelated files. GitHub's returned sizes and SHA-256
digests must match every planned asset before retention succeeds. A failed upload
cleans only the draft created by that invocation; an existing or published release
is not replaced. Signing, redistribution review and public release gates remain
independent requirements.

## 中文

交付卷仅用于维护传输及已有内部候选。用户发行必须是一个完整安装文件；Mac、Windows、
Linux完整工厂拒绝达到或超过单附件上限的安装包，不以分卷计作交付。维护格式将原始安装包按最多 1 GiB 分段，
逐卷及完整文件均使用 SHA-256 校验，清单摘要由发行方单独固定。Rust 程序负责分卷、
无额外全包副本的流式验证和原子还原；失败、取消或空间不足不发布半成品，也不覆盖
已有安装包。用户无需自行下载或还原分卷；维护命令不代表一键安装验收。
