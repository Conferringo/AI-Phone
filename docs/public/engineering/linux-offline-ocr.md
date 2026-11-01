# Offline Linux OCR

The Linux x64 OCR component provides Tesseract, its ELF dependency closure, English and Simplified Chinese language data, the TSV configuration, and Pillow for the bundled CPython 3.13 runtime. It is embedded in the native service component. It does not install Python packages or replace system libraries on the user's host.

## Execution and integrity

The phone runner keeps its existing PNG-to-regions interface. One canonical adapter owns image bounds, OCR parsing and confidence filtering. The packaged launcher selects the verified installer Python, isolates Python's import path and loads Pillow from the OCR component. Tesseract uses its included ELF loader and libraries for that process only. No global library-path change is required.

The factory archive is pinned in `config/linux-ocr.json`. Admission checks archive bytes, regular-file paths, size/count budgets, ELF architecture and required language/configuration files. The component inventory binds source identity, hashes and permissions. Native service admission includes every OCR file; invalid components fail explicitly. A valid bundled OCR component removes the corresponding apt prerequisites from installation. Other Linux host prerequisites remain separate. The `tessdata/configs/tsv` configuration is required, and the adapter rejects output without a valid TSV header instead of treating it as empty regions.

## Maintainer build

Create a dedicated factory context containing `scripts/linux-ocr/collect.py`, `scripts/linux_dependency_factory.py` as `factory.py`, and the Linux x64 `python.tar.gz` identified in `config/installer-tools.json`. Do not use a development checkout or user directory as the Docker build context.

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

## Availability

Current platform support, installer availability and distribution requirements are maintained in [release status](../release-status.md). Component formats and build interfaces do not independently qualify a complete installation.

## 中文

Linux x64 OCR 组件内置 Tesseract、ELF 动态依赖、英文与简体中文字库、TSV 配置和适配随包 CPython 3.13 的 Pillow。运行时仅为识别进程选择自带动态加载器与库，不替换用户系统库，也不在用户主机安装 Python 包。有效组件接入原生服务清单后，安装器不再通过 apt 重复安装 OCR。

TSV 配置属于必需资源，适配器拒绝无效 TSV 头，不把错误格式当成空识别结果。当前宿主范围、完整安装包供应和分发要求统一见[发行状态](../release-status.md)。
