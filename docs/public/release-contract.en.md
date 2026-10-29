# Release contract and gap audit

[简体中文](release-contract.zh-CN.md) · 2026-10-28 · Requirements, not a claim of release readiness.

## Decision

The intended product lets a user download an application, configure their own devices and Agents, and use an AI-operated Android phone and Linux computer. Public source, understandable technical documentation and installable software are separate deliverables. Today there is a local source archive and a macOS ARM64 native-client test package, not four complete deployment installers. Original code remains Apache-2.0; dependencies and assets retain their own terms.

No remote repository or source upload has been made. A technical preview may precede the full product if its platform limits and prerequisites are explicit. Progress is measured by accepted checks, not an unsupported percentage.

## Intended installation flow

1. Download a package matching the operating system and CPU architecture. Choose Local deployment or Connect to server. Never default to the developer's host.
2. Check memory, disk, virtualization and network; acquire permitted components, show required agreements/system permissions, and resume interrupted installation. Ordinary users should not install Rust/Node or compile source.
3. Create one Android and one Linux device by default. Choose verified system templates, CPU, RAM, disk and display identity. Display identity is not hardware attestation. Resizing an existing device and reinstalling it are distinct operations.
4. Configure the model/API once when creating an Agent, then bind devices, channels and memory. An Agent may control both devices; conversation columns remain per device. Skills, knowledge and work files are shared.
5. Receive real frames, input, audio, screenshots, grounded actions and cross-device files. Each guest includes its own graphical Clash app; the host defaults to a direct connection.
6. Check updates, back up, restore and uninstall through the application. Removing the application preserves user data; deleting that data is a separate explicit action.

The first Android client connects to the user's server. Hosting the entire system on an ordinary unrooted phone is a separate unverified goal. An installer may require operating-system permission or a reboot; it cannot bypass those requirements.

## Platform artifacts

These are targets, not available downloads. Record tested OS versions, architectures, minimum resources and physical-machine results for each.

| Platform | Target artifact | Local-server target | Current evidence and gaps |
| --- | --- | --- | --- |
| macOS | Signed, notarized DMG | Apple Silicon first; native Android and Docker services/Linux | ARM64 source builds and existing guests verified. Tauri .app/DMG preview now exists for an existing deployment; no Developer ID signing/notarization or clean physical-host server installation acceptance. Intel is not promised. |
| Windows | Signed NSIS installer or MSI; choose one primary entry | x64 first; native Android and WSL2/Docker | No installer or validated lifecycle, paths, permissions, OCR or virtualization adapter. ARM64 requires a separate assessment. |
| Linux | AppImage and deb first; rpm separately accepted later | x64 first; KVM Android, Docker and systemd | Linux source-check CI configuration is not full-server validation. Client, host OCR, discovery and installation are pending. ARM64 needs its own checks. |
| Android | Signed APK | Remote client initially | No client APK, pairing/TLS, touch or background-recovery acceptance. Existing IME/launcher APKs are guest helpers. |

Current templates are Android 15 ARM64 and Debian 13/Openbox ARM64. Fedora is unavailable. System-version choices, cross-architecture support and existing-device resizing are not complete product flows. ARM64 guest/app evidence cannot establish x64 Windows/Linux compatibility.

A Tauri 2 native client is implemented; a portable host-management layer remains open. The current Next.js application has server API routes, so a static export cannot package all existing behavior. The shell should connect to a managed local or user-owned server; native capabilities must have narrow command/origin scopes. A remote page must not acquire arbitrary host execution. [Tauri's Next.js guide requires static export](https://v2.tauri.app/start/frontend/nextjs/); its [distribution guide](https://v2.tauri.app/distribute/) treats platform builds and signing separately.

## Repository deliverables

| Area | Present | Remaining acceptance |
| --- | --- | --- |
| Source | Web, Rust, protobuf, channels, media, device bridges, infrastructure, built-in Skills, lockfiles and an allowlisted export | Source tags matching binaries, reproducible build instructions and actual remote CI runs |
| Installation | Source installation commands with checkpoints/retry; macOS native-client preview | Managed runtimes/downloads, resource/system wizard, other native platforms and interrupted clean-host installation |
| Data lifecycle | Separate user staging, database roles, consistent backup, quarantined restore and restored guest boots | Signed version manifests, in-app updates, compatibility checks, migration/activation, rollback and data-preserving uninstall |
| Real execution | Existing Android/Linux operation, target/OCR actions, action ledger, durable tasks and file round trips | Each platform's real device workflow, prolonged operation and host sleep/reboot recovery |
| Models/channels/languages | Two API styles, channel adapters and eight UI catalogs | Actual capability/attachment matrix and language input/error/narrow-screen acceptance |
| Documentation/legal | Bilingual README, setup, architecture, recovery, contributing, security, Apache LICENSE/NOTICE and dependency inventory | This contract, privacy/disclaimer drafts, binary/image notices, corresponding source, signatures and a standard SBOM |

A release page needs installers, source, SHA256, signatures/verification instructions, per-platform requirements, changelog and known issues. Artifacts must match their source version. Never include keys, accounts, conversations, guest disks, screenshots, attachments, private memory or subscriptions. The current `.github/workflows/source.yml` checks source only; it neither builds/signs four installers nor has a remote run record.

## Directory boundaries

| Path | Responsibility |
| --- | --- |
| `apps/web` | Next.js workbench and server forwarding |
| `apps/client` | Tauri native client and local connection settings |
| `apps/channels` | External messaging adapters |
| `apps/device-stream` | Frames, audio and media |
| `apps/device-network` | Guest Clash acquisition and integrity checks |
| `apps/android-input`, `apps/android-home` | Guest input/notification bridge and launcher |
| `crates/assistant` | Agent loop, tools, models, permissions and persistence |
| `crates/phone-runner` | Root ADB, targeting, OCR and execution |
| `crates/api`, `packages/protocol/proto` | Generated Rust types and protobuf source |
| `infra`, `config`, `skills` | Guest recipes, pinned presets/release gates and built-ins |
| `scripts`, `docs/public` | Development/maintenance/export and distributable documentation |
| `user`, `dist` | Private persistent state and generated artifacts; neither belongs in Git |

`apps/client` now implements the native client and macOS packaging; the portable host-manager remains unfinished. Platform work includes replacing launchd, Swift Vision OCR, Unix permission assumptions and resource management. Redis is an optional Compose service, not the Agent queue; an image declaration is not evidence of durable scheduling.

## Licence boundaries

Apache-2.0 fits permissive self-hosting and commercial integration. It allows closed-source downstream changes, offers a limited contributor patent grant, requires applicable notices/change disclosures, and grants no trademarks. It neither guarantees patent clearance nor relicenses dependencies. See the [official terms](https://www.apache.org/licenses/LICENSE-2.0).

GSAP has [custom restrictions](https://gsap.com/community/standard-license/). Clash/Mihomo and guest packages require copyleft/source review; separate processes alone do not settle derivative-work questions. [Apache's compatibility note](https://www.apache.org/licenses/GPL-compatibility.html) describes the distinction. Redis 8 is tri-licensed, so the pinned image needs an explicit applicable option and redistribution review, not an old BSD assumption: [Redis licences](https://redis.io/legal/licenses/). SDK artifacts and Docker Desktop have separate terms: [Android SDK](https://developer.android.com/studio/terms), [Docker Desktop](https://docs.docker.com/subscription-billing/desktop-license/).

See [third-party notices](../../THIRD_PARTY_NOTICES.md), [disclaimer](../../DISCLAIMER.md) and [privacy](../../PRIVACY.md). These engineering drafts do not replace applicable law or add incompatible restrictions to the Apache licence.

## Delivery order

1. Clear technical-preview gates: asset/distribution review, actual UI acceptance, clean Mac installation and host restart/prolonged recovery. Preserve existing passed database, backup and action-ledger evidence without resetting live data.
2. Deliver one real installer end to end: client plus constrained host manager, macOS ARM64 DMG, managed runtimes, default device creation, Agent setup, resumable install and data-preserving uninstall.
3. Add Linux server/client, Windows service/installer and Android remote client. Accept each CPU architecture independently.
4. Complete signed update, staging, backup/migration/health/rollback and interrupted-upgrade tests before exposing an update button.

Technical-preview publication and full four-platform product release have different acceptance lists. [Release gates](release-status.md) remain open. This audit does not mark planned work complete or authorize an upload.


2026-10-28 platform addendum: local macOS ARM64 DMG, Windows x64 NSIS, Linux ARM64 DEB and Android ARM64 APK now exist. These clients connect to an existing deployment; they do not satisfy the server-installer contract above. See [platform acceptance evidence](multiplatform-clients.md).
