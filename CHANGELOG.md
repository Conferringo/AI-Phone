# Changelog

## 0.1.4

- Download, verify, install and restart from the macOS and Windows client.
- Reuse the existing device disks, model configuration and conversations.
- Read macOS host capacity without starting command-line tools.
- Reduce model configuration work when loading the registry.

## 0.1.3

- Start new chat requests promptly and refresh saved activity as it arrives.
- Wake the Agent on device notifications and keep progress when instructions are added to a running task.
- Reduce Android observation work by reusing screen dimensions and reading notification data in parallel.
- Handle DeepSeek thinking options on Anthropic-compatible endpoints.
- Require sign-in for access over the local network.
- Keep the existing Windows device disk during updates and check for client releases from the app.

## 0.1.2

- Locate the bundled QEMU manifest during Windows setup so the installer can enable the hypervisor and start the local devices.
- Ship the startup, screen, input and audio fixes on Windows as well as macOS.

## 0.1.1

- Restore the macOS workbench after local services finish starting.
- Read CPU and memory settings from existing Android devices with spaced configuration entries.
- Keep phone and desktop frames current when switching devices or reconnecting.
- Deliver confirmed IME text once and preserve the order of desktop clicks and keyboard input.
- Keep each device’s audio active when switching views and resume playback after an interruption.

## 0.1.0

- macOS Apple Silicon and Windows 11 x64 clients with bundled Android and Linux devices.
- Phone and computer control, independent device conversations, notification wake and autonomous exploration.
- Shared Skills, linked knowledge and work files. Model, memory and channel configuration per Agent.
- Local installation and recovery, protobuf business API and PostgreSQL persistence.
- Ten interface languages, including Arabic with right-to-left layout.

[Downloads and release notes](https://github.com/MrMilkDragon/AI-Phone/releases)
