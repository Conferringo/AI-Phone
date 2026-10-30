# 客户端与服务发行审计 / Client and service release audit

2026-10-28 · `0.1.0-preview.1` · Local review only

本批产物是可连接现有部署的 macOS ARM64 客户端预览和源码审查包。尚不是全新机器免编译安装器，也不是四平台正式发行。原生包未做 Developer ID 签名或公证；没有创建远端或上传源码、安装包及用户状态。

This batch delivers a local macOS ARM64 client preview and source review bundle. It requires an existing server deployment. It is not a complete clean-host installer or a four-platform release. No remote publication, Developer ID signing or notarization has occurred.

## 已修复 / Fixed

| 范围 / Area | 原因与结果 / Cause and result | 验证 / Verification |
| --- | --- | --- |
| 原生服务器切换 / Native server switching | 导航回调捕获第一次服务器地址，导致保存新地址却仍停在旧页面。改为动态同源权限，并在导航失败时回退。 / Navigation captured the first origin; permissions now follow the selected server with rollback on failure. | 已安装客户端实际往返切换 localhost / 127.0.0.1，两次均到达正确工作台。 / Both directions passed in the installed client. |
| 原生启动与 IPC / Startup and IPC | 后台启动探测不得覆盖手动连接；本地连接页限定精确协议、主机、端口与路径，拒绝凭据、查询串、片段及新窗口。 / A cancelled startup probe cannot replace a manual connection. Native connection pages use exact URL validation. | 原生 4 项测试；启动竞态代码检查通过，未声称已做真实宿主启动竞态注入。 / Four native tests; actual slow-start race injection remains pending. |
| 连接失败 / Connection failure | 错误恢复地址焦点、标注无效字段；暗色错误可读。异步读取旧地址不覆盖正在输入的新地址。 / Restore focus and readable errors; do not overwrite user edits with a late saved-address response. | 错误端口 → 修改地址 → 连接成功，原生实测。 / Invalid-port recovery passed in the native app. |
| 原生快捷键 / Native shortcut | 电脑获得键盘焦点后，不再把 Command+, 映射为 Linux 的 Ctrl+,。 / Guest keyboard handling now preserves the native Command+, shortcut. | 真实电脑输入焦点 → Command+, → 原生连接窗口 → 返回工作台通过。 / Native connection shortcut and return to workbench passed with guest input focused. |
| 请求正文 / Request bodies | 六类 Web 桥接在读取中限制字节与 30 秒期限，禁止先无限缓冲再检查。 / Six Web bridge groups bound bytes during streaming and impose a 30-second receipt deadline. | 无/伪造长度、越界、取消、超时测试；真实 chunked 请求未结束即返回 413。 / Stream tests and a live unterminated chunked request returned 413 at the limit. |
| 页面响应头 / Response headers | 添加禁止嵌套、类型嗅探、来源泄漏及 object/base 限制。 / Add framing, MIME, referrer, object and base restrictions. | 实际响应头核验；跨域进入 403、匿名聊天历史 401。此 CSP 不是完整脚本策略。 / Live checks passed; this is not a full script CSP. |
| 知识图谱 / Knowledge graph | 标签按屏幕像素避让、限制边界；缓存布局，不逐帧重新排版。 / Place readable labels without collisions and cache layout between geometry changes. | 14 个长标题在 800/350 像素几何测试通过；真实 1440×980 与 960×720 客户端验收。 / Geometry tests and two native window sizes passed. |
| 知识编辑子窗 / Knowledge editor | 切换功能后恢复草稿时丢失父窗关系，点击输入会关闭窗口并落到设备。改为显式父窗关系，点击固定不被旧 hover 计时器撤销。 / Restored drafts lost their parent window; explicit ownership and pinned hover handling prevent click-through. | 新建草稿 → Skill → 知识库 → 编辑标题与多行中文通过。未保存测试草稿已清空。 / Switching, draft retention and continued editing passed; temporary inputs were cleared without saving. |
| 长任务故障测试 / Recovery fixture | 假模型错误地将请求数当作成功工具数。改为数已持久返回的成功结果，并在第 17 次响应前固定中断。 / The fake model confused request count with completed effects; it now counts durable successful results with a deterministic kill barrier. | 完整故障矩阵通过，56 份唯一工具结果；没有放松生产执行规则。 / Full fault suite passed with 56 unique durable effects; production safeguards were not relaxed. |

## 检查结果 / Checks

- Bun: **65 tests, 582 assertions, 0 failures**. TypeScript, protobuf lint/build and Rust workspace checks passed. Web production build passed.
- Python: **31 tests, 5 external-database tests skipped** in the default run; separate disposable PostgreSQL integration ran **8 Rust integration, 3 role and 6 backup/restore tests**, all passed.
- Rust server: **54 assistant + 25 runner tests** passed. Native client: **4 tests**, Clippy with `-D warnings` passed. Server Clippy completes with **49 unique maintenance/style suggestions** still recorded; it is not a zero-warning result.
- Worker process faults: SIGKILL, PostgreSQL restart/outage, SIGSTOP/resume, unknown-result refusal, retry persistence, partial SSE, exhausted quota and 56 durable effects passed. **Actual host sleep, reboot, physical power loss and prolonged multi-device soak are not verified.**
- Only the Web service was reloaded. Existing guests and assistant workers were not rebuilt or restarted for this audit. Original account, device and chat table fingerprints were compared before and after; no test messages were sent to a real model or contact.

## 依赖与分发 / Dependencies and distribution

[Machine-readable security report](dependency-security.json) records the advisory database revision and selected build targets. No blanket advisory ignore is used.

- Bun audit returned no advisories. The server lockfile includes RSA through optional SQLx MySQL dependencies; it is absent from the selected server build graphs. The [RSA advisory](https://rustsec.org/advisories/RUSTSEC-2023-0071.html) remains visible in the report.
- Native Tauri **2.11.6** includes the published [IPC capability fix](https://github.com/tauri-apps/tauri/security/advisories/GHSA-w28w-mhc8-qvjv). Remote workbench content has no native command grants; local connection commands also check window identity and origin. See [Tauri capability scope](https://tauri.app/security/capabilities/).
- Linux native **GLib 0.18.5** is still reachable and affected by [RUSTSEC-2024-0429](https://rustsec.org/advisories/RUSTSEC-2024-0429.html). **Linux native release remains blocked.** UNIC and other unmaintained dependencies stay visible as maintenance risks; macOS passing the target check does not mean all warnings are absent.
- Complete upstream notice supplements now cover **14 previously incomplete crate archives**, pinned to their source commit and SHA256. **26 complete-notice gaps remain** in the cross-platform native lockfile inventory. A repository overview or licence identifier alone is not counted as a complete licence text. See [licence inventory](dependency-licenses.json) and [supplement provenance](../../config/native-notice-supplements.json).
- macOS `.app` validation checks identifier, version, minimum OS, the exact allowed file set, legal-resource equality, credential patterns and developer-home paths. The DMG and each bundle file have hashes. This does not replace code signing, notarization or legal review.
- A separate [native CI workflow](../../.github/workflows/client.yml) is prepared for macOS ARM64 tests, advisory checks and unsigned local packaging. **It has not run remotely and publishes no artifacts.**

## 发行仍需完成 / Remaining release gates

1. 在独立干净宿主验证整套安装、设备创建、模型配置、文件交付、失败续装与卸载。当前 DMG 只安装客户端。 / Validate the whole product on a separate clean host; the DMG currently installs only the client.
2. 完成 Windows/Linux/Android 原生包与服务器平台适配、签名、公证及受验证的更新回滚。 / Finish platform packages, server adapters, signing and verified update/rollback.
3. 关闭许可文本、素材来源和二进制分发缺口，解决 Linux GLib 阻断。 / Close provenance and redistribution gaps, including the Linux GLib blocker.
4. 完成所有窗口、语言、附件、声音、失败场景、键盘可访问性及减少动效矩阵。此次知识库修复不代表全部 UI 已验收。 / Complete the full UI, language, media, error, accessibility and reduced-motion matrix.
5. 完成真实宿主睡眠/重启及长期多设备运行验证。 / Verify host lifecycle and prolonged multi-device operation.

[Release status](release-status.md) · [Native client](native-client.zh-CN.md) · [Release contract](release-contract.zh-CN.md) · [Database/backup/protobuf/Redis audit](release-audit-2026-10-28.md)

所有公开文档只包含脱敏结论。实际页面截图、聊天、设备磁盘、账号、密钥和运行日志留在私有工作目录，不进入源码或客户端包。 / Public material contains sanitized conclusions only; live screenshots and user state are excluded from source and client artifacts.
