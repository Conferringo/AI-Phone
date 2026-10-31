# Offline Linux OCR

The Linux x64 OCR component provides Tesseract, its ELF dependency closure, English and Simplified Chinese language data, the TSV configuration, and Pillow for the bundled CPython 3.13 runtime. It is embedded in the native service component. It does not install Python packages or replace system libraries on the user's host.

## Execution and integrity

The phone runner keeps its existing PNG-to-regions interface. One canonical adapter owns image bounds, OCR parsing and confidence filtering. The packaged launcher selects the verified installer Python, isolates Python's import path and loads Pillow from the OCR component. Tesseract uses its included ELF loader and libraries for that process only. No global library-path change is required.

The factory archive is pinned in `config/linux-ocr.json`. Admission checks archive bytes, regular-file paths, size/count budgets, ELF architecture and required language/configuration files. The component inventory binds source identity, hashes and permissions. Native service admission includes every OCR file; invalid components fail explicitly. A valid bundled OCR component removes the corresponding apt prerequisites from installation. Other Linux host prerequisites remain separate.

## Maintainer build

Create a dedicated factory context containing `scripts/linux-ocr/collect.py` and the Linux x64 `python.tar.gz` identified in `config/installer-tools.json`. Do not use a development checkout or user directory as the Docker build context.

```sh
docker buildx build --platform linux/amd64 \
  --target export --output type=local,dest=/factory/ocr-archives \
  -f scripts/linux-ocr/Dockerfile /factory/ocr-context

python3 scripts/linux_ocr_runtime.py \
  --archive /factory/ocr-archives/linux-x64-ocr.tar.gz \
  --destination /factory/ocr
```

The factory verifies the pinned Python archive and Pillow wheel, records distribution package versions and native file hashes, and retains package copyright files and wheel notices. Distribution repositories can change; a changed output requires a new reviewed archive pin, rather than accepting different bytes under an existing identity. Complete binary redistribution and corresponding-source review remain release requirements.

On the matching Linux host, pass `--linux-ocr /factory/ocr` with `--with-service-runtime` and `--installer-tools /factory/tools` to `scripts/build_client.py`. The OCR verifier requires validated portable installer tools and CPython 3.13. A factory component alone does not constitute a complete Linux client.

## Qualification

The 2026-10-31 factory produced a 43,672,441-byte archive and 392 inventoried runtime files, plus the component manifest. In a read-only Ubuntu container with networking disabled, no system Python or Tesseract installed, and only the selected component and pinned Python mounted, OCR recognized both `7319` and `微信工作7319`. The diagnostic image dimensions and 2,048-cell observation grid matched the adapter contract. This execution used x64 emulation on Apple Silicon; it is not a physical Linux/KVM installation result.

That check exposed a missing `tessdata/configs/tsv` dependency: Tesseract had exited successfully with plain text, which the adapter treated as empty regions. The component now requires the configuration, and the canonical adapter rejects an invalid TSV header explicitly. This failure is covered by regression tests.

QEMU, offline container-host provisioning, the full Linux client and other host platforms retain their own qualification gates. See [release status](../release-status.md).

## 中文

Linux x64 OCR 组件内置 Tesseract、ELF 动态依赖、英文与简体中文字库、TSV 配置和适配随包 CPython 3.13 的 Pillow。运行时仅为识别进程选择自带动态加载器与库，不替换用户系统库，也不在用户主机安装 Python 包。有效组件接入原生服务清单后，安装器不再通过 apt 重复安装 OCR。

断网、只读且没有系统 Python/Tesseract 的隔离容器中，包内运行环境已识别数字与中文测试图。缺少 TSV 配置曾导致成功退出却返回空结果；配置已补齐，适配器也会明确拒绝错误格式。该验证使用 Apple Silicon 上的 x64 模拟执行，不代表物理 Linux/KVM 全新安装通过。QEMU、容器宿主、完整客户端和二进制再分发继续独立验收。
