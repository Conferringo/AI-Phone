# Portable installer tools

The native client can carry a reviewed Python and Node runtime in `host/tools`. It authenticates their manifest against its compiled resource inventory before executing Python. A complete toolchain uses these files directly; it does not bootstrap Python through Mise or ask users to install Node. Contributor/source installations retain the managed-tool path.

## Ownership and integrity

`config/installer-tools.json` pins architecture-specific factory archives by URL, size and SHA-256. `scripts/installer_tools.py` builds the resource tree. Python's contained aliases become regular files; Node contributes its executable and licence/readme, without npm or Corepack. No personal interpreter installation or package cache is an input.

The manifest declares host, versions, executable paths, hashes, sizes and permissions. Native activation accepts only inventoried regular files, checks all bytes, and copies to `components/installer-tools/<manifest-sha256>` in the client's private installation. Source resources and user state remain separate. The limits are 12,000 files, 24,000 visited members, depth 64, 512 MiB of file bodies and a 4 GiB recovery reserve.

An existing immutable runtime must match exactly. A competing publisher can be reused only after its manifest and files verify. Cancellation, copy errors and Rust unwinding reclaim the publisher's own staging directory. Partial state never becomes an active runtime. Old immutable versions remain available to their existing installations.

The worker starts Python with isolated imports and bytecode writes disabled. After source staging, a verified private selection supplies the Python/Node paths to service launchers. Bundled executables precede host/project alternatives. With portable tools, native services and Android resources supplied, the installation tool list is empty and no package manager runs for those tools. Container-host, application and service dependency delivery remain independent distribution requirements.

## Maintainer build

Obtain and verify the factory archives specified by the matching host entry. Keep them in an owned build cache as `python.tar.gz` and `node.tar.gz`, then run:

```sh
python3 scripts/installer_tools.py \
  --archives /path/to/verified-archives \
  --output /path/to/new-tool-component

python3 scripts/build_client.py --platform macos \
  --with-web-runtime --with-service-runtime --with-phone-apps \
  --android-runtime /path/to/android.payload \
  --images-runtime /path/to/images.payload \
  --installer-tools /path/to/new-tool-component
```

These are contributor build commands, not the intended product setup. The completed distribution must already contain its runtime components. See [distribution status](../release-status.md).

## Public vendor build paths

The official macOS Node executable contains its upstream release-builder paths. Its exact executable SHA-256 is separately pinned after comparison with the authenticated upstream archive. The artifact scanner permits those public paths only for that exact resource and those exact bytes. Changed binaries and unrelated home paths fail. Raw and UTF-16 credential-pattern detection remain enabled for every file, including the approved vendor binary. Resource receipts are reused during expanded-bundle inspection instead of scanning the same verified resource again.

This distinction is about public vendor provenance. It does not approve private state or resolve third-party redistribution obligations. Python/Node licences remain with their components; complete binary publication requires the full notice and source review.

## Scope

The development baseline is Python 3.13.18 and Node 22.23.4. The macOS ARM64 component has 1,660 files and 213,170,787 bytes. In an OS sandbox denying all network access, the native installer copied and authenticated the actual component, then executed its Python with isolated imports and no host-tool PATH. The worker expanded the source payload, restored the private runtime selection, executed the included Node and retained an isolated chat fixture. SSL and SQLite execution also passed. A socket probe confirmed that network access was denied. This is component acceptance, not a complete two-device installation or a rebuilt product App.

The Python component boundary passed the local complexity/coverage gate (CC ≤9, CRAP <10), with no duplication candidates at similarity ≥0.8. Targeted mutation checks detected 162 of 164 changes; the two survivors change JSON key sorting. The native module has CC ≤9; native coverage and CRAP were not measured. Cancellation, changed sources, directory depth, exclusive staging and competing publication have regression tests. These results apply to this boundary, not the whole repository.

The matching Linux x64 factory archive pins are recorded; complete platform runtime acceptance is tracked separately. Windows and privileged Android independent hosts remain incomplete. The optional [input/launcher application component](phone-apps.md) supplies prebuilt APKs; fresh-device installation, container supervision, media and channel components also remain separate completion gates.

Primary sources: [Python portable distributions](https://docs.astral.sh/python-build-standalone/distributions/), [upstream release](https://github.com/astral-sh/python-build-standalone/releases/tag/20261026), [Node release checksums](https://nodejs.org/dist/v22.23.4/SHASUMS256.txt).

---

## 中文

原生客户端可随包携带 Python 与 Node，校验后直接启动安装程序；完整工具链不通过 Mise 联网准备解释器，也不要求用户安装 Node。组件来自固定摘要的官方归档，按清单复制到独立、不可变的私有目录。工具、应用版本和用户设备/聊天数据各自保存。

原生复制会检查全部文件、权限、架构和空间，取消、异常及并发发布均保持内容验证和暂存目录归属。服务优先使用选定的随包程序。Node 组件不含 npm/Corepack；公开上游构建路径仅按精确的已审阅文件摘要区分，凭据扫描保持开启。

当前 macOS ARM64 组件包含 1,660 文件、213,170,787 字节。在系统拒绝全部网络的沙箱中，原生安装器真实复制、校验随包程序，再由隔离的 Python 3.13.18 展开源码、恢复运行路径并调用 Node 22.23.4；SSL、SQLite 和隔离聊天夹具保持均通过。网络探测确认访问被拒绝。该证据属于组件验证，并非双设备完整初始化或最终 App 验收。

Python 边界的 CC≤9、CRAP<10，0.8 相似度无重复候选；164 项定向变异捕获 162 项，另两项改变 JSON 键排序。原生模块 CC≤9，覆盖率与 CRAP 尚未测量，不外推全仓。Linux x64 已固定对应归档。完整容器宿主、手机输入/桌面应用、媒体/渠道组件、各平台初始化和二进制声明仍继续推进；这个组件不单独代表完整客户端发行。
