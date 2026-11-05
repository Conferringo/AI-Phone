# Portable installer tools

The native client carries a reviewed installer runtime in `host/tools`. When both Web and JavaScript services are prebuilt for Bun, the client includes Python only: it neither carries, checks nor downloads host Node. Contributor/source installations retain the Python/Node toolchain. The native client authenticates the tool manifest against its compiled resource inventory before executing Python.

## Ownership and integrity

`config/installer-tools.json` pins architecture-specific factory archives by URL, size and SHA-256. `scripts/installer_tools.py` builds the full maintainer tool tree; client preparation selects the tools needed by its bundled components. Python's contained aliases become regular files. When Node is needed, its executable and licence/readme are included without npm or Corepack. No personal interpreter installation or package cache is an input. Existing two-tool manifests remain readable; an installer manifest always requires Python.

The manifest declares host, versions, executable paths, hashes, sizes and permissions. Native activation accepts only inventoried regular files and checks their bytes as it copies them to `components/installer-tools/<manifest-sha256>` in the client's private installation. Reuse verifies the installed files and does not hash unused source binaries. Source resources and user state remain separate. The limits are 12,000 files, 24,000 visited members, depth 64, 512 MiB of file bodies and a 4 GiB recovery reserve.

An existing immutable runtime must match exactly. A competing publisher can be reused only after its manifest and files verify. Cancellation, copy errors and Rust unwinding reclaim the publisher's own staging directory. Partial state never becomes an active runtime. Old immutable versions remain available to their existing installations.

The worker starts Python with isolated imports and bytecode writes disabled. After source staging, a verified private selection supplies only the declared tool paths to service launchers. Bundled executables precede host/project alternatives. With all prebuilt components and portable tools supplied, the installation tool list is empty and no package manager runs for those tools. Container-host, application and service dependency delivery remain independent distribution requirements. The same tool selection applies to the prebuilt WSL backend; development tools inside the Linux device remain unchanged.

## Maintainer build

Obtain and verify the factory archives specified by the matching host entry. Keep them in an owned build cache using the pinned filenames (`python.tar.gz` and the matched Node archive), then run:

```sh
python3 scripts/installer_tools.py \
  --archives /path/to/verified-archives \
  --output /path/to/new-tool-component

python3 scripts/build_client.py --platform macos \
  --with-web-runtime --with-js-services --with-service-runtime --with-phone-apps \
  --android-runtime /path/to/android.payload \
  --images-runtime /path/to/images.payload \
  --installer-tools /path/to/new-tool-component
```

These are contributor build commands, not the intended product setup. The completed distribution must already contain its runtime components. See [distribution status](../release-status.md).

## Native host layouts

| Host | Python entry | Node entry | Factory input |
|---|---|---|---|
| macOS ARM64 / Linux x64 | `python/bin/python3` | `node/bin/node` | Pinned tar archives |
| Windows x64 | `python/python.exe` | `node/node.exe` | Pinned Python tar and selective Node ZIP |

The [installer factory](../../../scripts/installer_tool_factory.py) downloads the reviewed archives into a fresh owned directory, builds the component and executes its programs under an isolated profile. Python runs with isolated imports and bytecode writes disabled; SSL, SQLite and native-library loading are checked. Node contributes its executable, licence and README. Windows resource metadata follows native filename, case, alias and permission rules shared with the Android component.

The [Windows qualification workflow](../../../.github/workflows/windows-installer-tools.yml) records the component inventory and actual program versions. This runtime is part of the Windows host installer; the independent device/backend environment has separate qualification.

## Public vendor build paths

The official macOS Node executable contains its upstream release-builder paths. Its exact executable SHA-256 is separately pinned after comparison with the authenticated upstream archive. The artifact scanner permits those public paths only for that exact resource and those exact bytes. Changed binaries and unrelated home paths fail. Raw and UTF-16 credential-pattern detection remain enabled for every file, including the approved vendor binary. Resource receipts are reused during expanded-bundle inspection instead of scanning the same verified resource again.

This distinction is about public vendor provenance. It does not approve private state or resolve third-party redistribution obligations. Python/Node licences remain with their components; complete binary publication requires the full notice and source review.

## Availability

Current platform support, installer availability and distribution requirements are maintained in [release status](../release-status.md). Component formats and build interfaces do not independently qualify a complete installation.

The configured baseline versions are Python 3.13.18 and Node 22.23.4. The optional [input/launcher component](phone-apps.md) supplies the Android control applications.

Primary sources: [Python portable distributions](https://docs.astral.sh/python-build-standalone/distributions/), [upstream release](https://github.com/astral-sh/python-build-standalone/releases/tag/20261026), [Node release checksums](https://nodejs.org/dist/v22.23.4/SHASUMS256.txt).

---

## 中文

Web 与 JavaScript 服务均预编译为 Bun 组件时，原生客户端和 WSL 后端只携带安装所需的 Python，不携带、检查或下载宿主 Node。源码安装保留 Python/Node，已有双工具清单仍可读取；Linux 电脑内的开发工具不变。组件来自固定摘要的官方归档，按清单复制到独立、不可变的私有目录。工具、应用版本和用户设备/聊天数据各自保存。

原生复制会检查全部文件、权限、架构和空间，取消、异常及并发发布均保持内容验证和暂存目录归属。服务优先使用选定的随包程序。Node 组件不含 npm/Corepack；公开上游构建路径仅按精确的已审阅文件摘要区分，凭据扫描保持开启。

随包版本由配置与组件清单确定，当前基线为 Python 3.13.18 与 Node 22.23.4。平台范围、安装包供应和完整初始化条件统一见[发行状态](../release-status.md)。


## Public cryptographic strings

Artifact inspection recognizes only the exact public byte spans pinned in `config/public-crypto-vectors.json`. GnuTLS self-test digests are bound to its [immutable upstream source](https://github.com/gnutls/gnutls/blob/2f04c14daa086889b4e85f7056c39d23766ddaa1/lib/crypto-selftests-pk.c). The libssh entries bind its OpenSSH format marker together with the following parser diagnostic, and three public security-key algorithm suffixes, to the [official 0.10.6 source archive](https://security.ubuntu.com/ubuntu/pool/main/libs/libssh/libssh_0.10.6.orig.tar.xz). Every span includes its terminating NUL; the marker alone is not accepted. The catalog records source-file digests, symbols and encoded sizes without key bytes.

Recognition affects scanning only. Bundled file bytes and their integrity receipts remain unchanged. The scanner retains a bounded suffix across chunks so it can verify an entire test string before classifying it. Changed strings, unknown private keys and adjacent API credentials remain failures, including UTF-16 credentials. Neither a library filename nor a public binary digest exempts a complete file from secret detection.
