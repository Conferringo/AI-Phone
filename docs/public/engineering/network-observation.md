# Device network observation

Clash runs as an application inside each device. Its application owns its
configuration and lifecycle; the host does not restart or reconfigure it during
routine recovery. Network observation is separate from agent GUI interaction.

The Rust host runtime owns network state, explicit credential reads, core-health
reads and device-originated probes. The authenticated assistant routes retain
their existing protobuf requests and responses. Phone reads use the serial-scoped
native ADB client. Desktop reads use the selected Docker command and guest tools.
They do not require Python in the guest or a Bun process for each API read.

Existing `source.json`, `job.json`, `checks.json` and application-handoff markers
remain in their device-owned directory. Observation does not rewrite subscription
or task records. State omits subscription credentials; the existing explicit
credential operation returns them. Unavailable guest state is reported as unknown,
not confirmed stopped. Legacy core observations retain the difference between an
applying, interrupted and committed request, and between stale and current checks.

Probe requests originate in the device, with host HTTP proxies bypassed. Android
probes run as UID 2000 and follow the device's application network route. HTTPS
and a pinned download are checked separately; one failed check does not erase
the others. Desktop diagnostic responses exceeding the existing 1 MiB read
budget now fail that check rather than publish a truncated download. This
diagnostic budget does not restrict browser or file-manager downloads.
Completed diagnostic records are published by rename. Probe files
are temporary on Android; the desktop retains its existing diagnostic download.
No probe sends a message or performs GUI work.

The Rust controller also owns legacy-core start, stop, resume and handoff to the
device application. Assistant recovery calls it directly. The private native
command uses the same lifecycle policy. Unknown process state cannot
authorize another launch, and application-managed resume does not restart the
device or change its saved configuration. The existing 210-second operation
deadline and guest startup/teardown polling budgets remain unchanged.

The native command owns cancellation and its guest process. Cancellation does
not certify that a preceding guest mutation was undone; callers observe current
state before another action. Application-resource selection, integrity admission
and streaming acquisition also belong to the Rust host component. Subscription
configuration is performed in the device's Clash application. The assistant API
exposes state, explicit credential reads and probes, without a second subscription
editor or configuration service. Bundled third-party guest tools are separate
from AI Phone's runtime implementation.

CLI contracts cover private-state preservation, application ownership, unknown
results, legacy journals, probe publication and actual local HTTP/Curl transport.
Disposable command fixtures isolate unit tests from user devices. Real guest
network checks are a separate integration qualification and do not by themselves
qualify browser rendering, notification delivery or a complete installer.
