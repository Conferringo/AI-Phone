# Local setup and configuration

[简体中文](getting-started.zh-CN.md) · Source installer preview, 2026-10-28

## Prerequisites

The verified server is macOS Apple Silicon, with native Android Emulator (Android 15 ARM64) and a Docker Debian 13/Openbox desktop. Install Node.js 22+, npm, Rust 1.88+, Python 3.11+, Java 17, Xcode Command Line Tools, Docker Desktop and Android command-line tools with sdkmanager. Recommend 16GB+ RAM and 35GiB free disk for fresh images/builds. Other server hosts are not verified.

## First run

1. Run `./ai-phone doctor` and resolve failed prerequisites; it does not modify devices.
2. Run `./ai-phone install`. Configure SDK/JDK paths, optional host/container HTTP proxies, new-device resources and language. Empty means direct networking; `-` clears a saved value.
3. Review the SDK's interactive licence prompt. The installer never automatically accepts it. It then downloads SDK packages/images, installs dependencies, builds workspace/desktop images and starts local services.
4. Open the client or local URL directly into the workbench; there is no welcome screen. Existing owner credentials and sessions are preserved; no administrator account or password is shown. Configure the first Agent model in Configuration.
5. In Configuration → Agent, enter a model ID, API URL and key. Changed settings require text, vision and tool round-trip probes. Authorize devices and assign each default Agent. Stored keys are visible/editable to the administrator.
6. Startup scripts create the primary phone. Create/install the computer in Configuration. Additional Android browser/APKs still need a pinned, licensed distribution workflow; a newly created phone must not be assumed to contain all apps.

This is guided source setup with prerequisites. Downloads and builds can fail; repair the reported issue and rerun. Existing AVDs and database passwords are preserved. Full installation rollback is not implemented.

## Resources and networking

`configure` saves a private 0600 host profile in `data/deployment/profile.json`. Creation presets do not resize existing devices. Desktop CPU/memory is separately configured in its creation form; its host-backed directory has no enforced independent disk quota.

New deployments default to direct networking. Existing installations without a profile retain legacy `.env` proxies. The installer downloads and verifies pinned native Clash apps. Import subscriptions and enable VPN/TUN inside each guest’s own app. No nodes are bundled; see [network configuration and verification](network.md).

Use `start`, `open` and `stop` for normal operation. Repeated start preserves existing services; partial-service recovery still needs diagnostics. LAN address discovery uses private interfaces; the address is saved in private `data/deployment.json`. LAN HTTP is unencrypted: use trusted TLS or a user-managed VPN on untrusted networks. No public tunnel is created.

## Review bundles

```sh
./ai-phone package
./ai-phone stage --archive dist/ai-phone-0.1.0-preview.1-source.tar.gz --destination /path/to/AIPhone
```

Bundles contain allowlisted source, a manifest and SHA256, excluding private state. Current bundles are internal/not for publication. Staging verifies files and keeps persistent user data separate. Only the first installation receives a current link; subsequent versions are staged without live activation. Checksums are not publisher signatures. See [distribution and updates](distribution.en.md).

## Personal entry and guest apps

The default local single-owner profile uses one Enter button and preserves existing credentials/sessions. Clients able to reach this trusted-LAN deployment can enter as its owner. This is not multi-user authentication. Set `local_entry` to false in the private deployment profile to disable this entry endpoint; existing authentication APIs remain available. Configure the first Agent model in Configuration.

Both guest creation flows install native Clash apps. Import subscriptions and manage nodes, VPN/TUN and startup inside each guest. The web configuration center does not manage guest networks. No developer subscription is distributed.

Installation now has private durable per-phase checkpoints and a process lock. Rerunning install verifies source inputs and artifacts before reusing completed work; start resumes missing owned services and preserves running tasks. It is not an automatic updater or whole-install transaction rollback. See [task recovery and retry behavior](task-recovery.md).
