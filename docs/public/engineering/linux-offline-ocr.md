# Offline Linux OCR

The Linux x64 OCR component provides Tesseract, its ELF dependency closure, English and Simplified Chinese language data, and the TSV configuration. It is embedded in the native service component. The Rust OCR runtime invokes the bundled native tool and models.

## Execution and integrity

The phone and desktop use the same Rust PNG-to-regions pipeline. Rust owns image bounds, screen signatures, TSV parsing and confidence filtering. The native helper selects the included ELF loader, Tesseract libraries and language data for that process only. No global library-path change is required.

The factory archive is pinned in `config/linux-ocr.json`. Admission checks archive bytes, regular-file paths, size/count budgets, ELF architecture and required language/configuration files. The component inventory binds source identity, hashes and permissions. Native service admission includes every OCR file; invalid components fail explicitly. A valid bundled OCR component removes the corresponding apt prerequisites from installation. Other Linux host prerequisites remain separate. The sealed archive inventory includes `tessdata/configs/tsv`; the Rust helper selects TSV output directly with `tessedit_create_tsv=1`. It rejects output without a valid TSV header instead of treating it as empty regions.

## Maintainer build

The TypeScript `dependencyContext` producer creates a dedicated context with the OCR Dockerfile and collection tooling. It copies the bootstrap CA package from `config/linux-factory.json` and writes the selected snapshot to `ubuntu.sources`. Do not use a development checkout or user directory as the Docker build context.

OCR and QEMU dependency factories use the same pinned [Ubuntu archive snapshot](https://snapshot.ubuntu.com/). The minimal base image receives the reviewed Ubuntu CA certificates before accessing that snapshot over HTTPS; TLS verification, signed APT metadata and exact output archive checks remain enabled. A failed index update stops the build. These operations run on the maintainer's builder, not on an end user's host during client startup.

```sh
docker buildx build --platform linux/amd64 \
  --target export --output type=local,dest=/factory/ocr-archives \
  -f scripts/linux-ocr/Dockerfile /factory/ocr-context

./scripts/bun scripts/linux-dependency.ts --role linux-ocr \
  --archive /factory/ocr-archives/linux-x64-ocr.tar.gz \
  --destination /factory/ocr
```

The factory runs its TypeScript collector with the pinned Bun build executable. It records distribution package versions and native file hashes, and retains package copyright files. Updates to the shared snapshot require verifying both dependency archives; a changed output requires a new archive pin, rather than accepting different bytes under an existing identity. Complete binary redistribution and corresponding-source review remain release requirements.

The matching Linux OCR component is assembled into the Windows WSL backend. Its Rust helper and native dependency closure are qualified separately from the Windows client installer.

## Availability

Current platform support, installer availability and distribution requirements are maintained in [release status](../release-status.md).

## 中文

Linux x64 OCR 组件内置 Tesseract、ELF 动态依赖、英文与简体中文字库和TSV配置。Rust负责PNG解码、画面摘要和结果解析，为识别进程选择自带加载器、库及字库。构建生产者使用 TypeScript 和固定 Bun 收集依赖及许可。有效组件接入原生服务清单后，安装器不再通过 apt 重复安装 OCR。

封装清单包含 TSV 配置文件；Rust 通过 `tessedit_create_tsv=1` 直接选择输出格式，不依赖该文件启用 TSV。适配器拒绝无效 TSV 头，不把错误格式当成空识别结果。当前宿主范围、完整安装包供应和分发要求统一见[发行状态](../release-status.md)。
