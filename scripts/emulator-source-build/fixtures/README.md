# Pinned small-file test fixtures

These two files are verbatim small inputs from AOSP Emulator commit `d1bfd6eb0ea9f697dc4cc1d258887c8b4220b155`. Exact upstream URLs, Git blob identities and SHA-256 digests are in [provenance.json](provenance.json). Tests read their bytes or parse the syntax tree; they do not import or execute upstream modules.

`toolchains.py` preserves its Android Open Source Project copyright and Apache-2.0 header. The Apache-2.0 terms are supplied in the project [LICENSE](../../../LICENSE) and at the [official licence text](https://www.apache.org/licenses/LICENSE-2.0). `source.properties` is the upstream package-revision metadata. These fixtures do not grant rights to unrelated SDK binaries, firmware or other Emulator dependencies.
