# Offline Linux OCR

The Linux x64 OCR component provides Tesseract, its ELF dependency closure, English and Simplified Chinese language data, and the TSV configuration. It is embedded in the native service component. The Rust OCR runtime invokes the bundled native tool and models.

## Execution and integrity

The phone and desktop use the same Rust PNG-to-regions pipeline. Rust owns image bounds, screen signatures, TSV parsing and confidence filtering. The native helper selects the included ELF loader, Tesseract libraries and language data for that process only. No global library-path change is required.

The component manifest records the factory recipe, archive digest, file hashes and permissions. The archive includes `tessdata/configs/tsv`. The Rust helper selects TSV output with `tessedit_create_tsv=1` and checks its header.

The [native OCR fixture](../../../crates/phone-runner/fixtures/ocr-probe.png) renders `7319`. The [source asset inventory](../../../config/source-assets.json) records its font and provenance. The separate [provider fixture](../../../crates/assistant/fixtures/vision-probe.png) exercises model image input.

## Maintainer build

The OCR and QEMU factories use a pinned Ubuntu base image, CA package and [archive snapshot](https://snapshot.ubuntu.com/). The collector records package versions and native file hashes, and includes each package's copyright file.

```sh
./scripts/bun scripts/linux-dependency.ts --role linux-ocr \
  --work /factory/ocr-context \
  --destination /factory/ocr
```

The command builds the x64 closure in a dedicated Docker context and stages it with its manifest. The Windows backend embeds that closure with the Rust OCR helper.

## Availability

Current platform support, installer availability and distribution requirements are maintained in [release status](../release-status.md).

## 中文

Linux x64 OCR 组件内置 Tesseract、ELF 动态依赖、英文与简体中文字库和TSV配置。Rust负责PNG解码、画面摘要和结果解析，为识别进程选择自带加载器、库及字库。构建生产者使用 TypeScript 和固定 Bun 收集依赖及许可。有效组件接入原生服务清单后，安装器不再通过 apt 重复安装 OCR。

封装清单包含 TSV 配置文件；Rust 通过 `tessedit_create_tsv=1` 直接选择输出格式，不依赖该文件启用 TSV。适配器拒绝无效 TSV 头，不把错误格式当成空识别结果。当前宿主范围、完整安装包供应和分发要求统一见[发行状态](../release-status.md)。
