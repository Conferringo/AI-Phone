# Local setup and configuration

[简体中文](getting-started.zh-CN.md) · Contributor source setup

## Choose an installation path

Check [release status](release-status.md) for installer availability and platform qualification before downloading. The intended product setup is to install the client, choose language and device resources, and configure a model; see the [integrated-host contract](integrated-hosts.md).

The instructions below build the project from source on Apple Silicon. For the Windows package, see the [Windows factory](windows-client-factory.md).

## Update the installed client

When an update is available, the workbench shows an update icon at the top right. Open it to read the version, release name and notes, then select **Install Update**. AI Phone verifies the package, installs it and reopens using your existing devices, model settings and conversations. Windows may show a system approval prompt.

## Source alternative: prerequisites

This source path uses macOS Apple Silicon, native Android Emulator (Android 15 ARM64) and a Docker Debian 13/Openbox desktop. Install Node.js 22+, Bun (project bootstrap), Rust 1.95+ (release builds pin 1.96.0), Java 17, Xcode Command Line Tools, Docker Desktop and Android command-line tools with sdkmanager. Recommend 16GB+ RAM and 35GiB free disk for fresh images/builds.

## First run

1. Run `./ai-phone doctor` and resolve failed prerequisites; it does not modify devices.
2. Run `./ai-phone install`. Configure SDK/JDK paths, optional host/container HTTP proxies, new-device resources and language. Empty means direct networking; `-` clears a saved value.
3. Review the SDK's interactive licence prompt. The installer never automatically accepts it. It then downloads SDK packages/images, installs dependencies, builds workspace/desktop images and starts local services.
4. Open the client or local URL directly into the workbench; there is no welcome screen. Existing owner credentials and sessions are preserved; no administrator account or password is shown. Configure the first Agent model in Configuration.
5. In Configuration → Agent, enter a model ID, API URL and key. Changed settings require text, vision and tool round-trip probes. Choose the profile that both devices will use. Stored keys are visible/editable to the administrator.
6. Startup scripts create the primary phone. Create the computer in Configuration. Install extra Android apps in the phone.

This is guided source setup with prerequisites. After resolving a reported download or build error, run `./ai-phone install` again. Existing AVDs and database passwords are preserved. Full installation rollback is not implemented.

## Resources and networking

`configure` saves a private 0600 host profile in `data/deployment/profile.json`. Creation presets do not resize existing devices. Desktop CPU/memory is separately configured in its creation form; its host-backed directory has no enforced independent disk quota.

New deployments default to direct networking. Existing installations without a profile retain legacy `.env` proxies. The installer downloads and verifies pinned native Clash apps. Import subscriptions and enable VPN/TUN inside each guest’s own app. No nodes are bundled; see [network configuration and verification](network.md).

Use `start`, `open` and `stop` for normal operation. Repeated start preserves existing services; partial-service recovery still needs diagnostics. LAN address discovery uses private interfaces; the address is saved in private `data/deployment.json`. LAN HTTP is unencrypted: use trusted TLS or a user-managed VPN on untrusted networks. No public tunnel is created.

## Review bundles

Use the archive path printed by `package` in the `stage` command.

```sh
./ai-phone package
./ai-phone stage --archive /path/to/generated-source-bundle.tar.gz --destination /path/to/AIPhone
```

The source archive includes a file manifest and SHA-256 checksum. Staging verifies the files and keeps user data in a separate directory. The first installation creates the current link. Later versions are staged for activation. See [distribution and updates](distribution.en.md).

## Personal entry and guest apps

The default local single-owner profile opens the workbench directly and preserves existing credentials/sessions. Clients able to reach this trusted-LAN deployment can enter as its owner. This is not multi-user authentication. Set `local_entry` to false in the private deployment profile to disable this entry endpoint; existing authentication APIs remain available. Configure the first Agent model in Configuration.

Both guest creation flows install native Clash apps. Import subscriptions and manage nodes, VPN/TUN and startup inside each guest. The web configuration center does not manage guest networks. No developer subscription is distributed.

Installation stores private durable per-phase checkpoints and uses a process lock. On retry, install verifies source inputs and artifacts before reusing completed work; start resumes missing owned services and preserves running tasks. See [task recovery and retry behavior](task-recovery.md).

## Liberty (development runtime)

Enable Liberty in the conversation of a configured device. The Agent can choose and revise its own goals. Reply and installation settings still apply. A paused owner task keeps its own history. The Agent can save reusable methods after checking the evidence. See [Liberty](autonomous-exploration.md) and [notification-driven work](notification-workflows.md).
