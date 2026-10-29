# CSS and source preview review · 2026-10-29

## Changes

- One radius scale: controls 8px, windows/popovers 14px, small indicators 6px. Device hardware outlines and circular controls retain their geometry.
- Shared floating-window styling moved to `apps/web/src/styles/window.css`; animation reads the computed radius. Removed duplicate shell styling and redundant CSS entrance animation.
- Authentication, initialization and error recovery share `styles/entry.css`. Local mode still opens the workbench directly; `/login` redirects. Password/setup fallbacks remain for deployments that need them.
- First-run configuration contains Agent/model, devices and a final connection check. Connections and memory stay available in the ordinary Agent editor, outside onboarding.
- Registry CSS duplicate selectors/media blocks consolidated without changing data ownership. Native connection defaults to 480×300, with a centered content group, 24px section spacing and 8px field spacing. Error messages belong to their field.

## Evidence and limits

The real macOS native WebView displayed the production frontend against a disposable protobuf fixture. Screenshots were reviewed privately for first-run configuration, password fallback, administrator setup and the native connection window. No saved API key, private conversation or account was included in screenshots. Incorrect password feedback was visible and input remained intact. Native connection failure retained the address and offered retry. First-run navigation no longer includes connection/memory sections.

A stale CSS manifest in the temporary Next process initially produced an unstyled page. Restarting that process after the completed production build fixed it; the corrected screenshots are the acceptance evidence. The production web service was restarted against the final build as well.

No real administrator was created or password changed. Fixture responses are not real model/channel/device execution. Existing real devices and conversations were preserved. Browser automation is disabled on this workstation; this batch used an isolated native review app. New small-screen/touch/screen-reader and cross-platform checks remain unverified. Reduced-motion/forced-color CSS is present; no new complete OS preference matrix is claimed.

Targeted checks include Bun regression tests, TypeScript, Next production build, Python source/export tests, native build and CSS parsing. Full-repository SOLID completion is still open; see [audit](solid-audit.md).

## Publication boundary

The maintainer authorized an experimental public source preview and explicitly chose to stop waiting for separate-device installation verification. This is a publication decision, not successful installation evidence. The false acceptance gate remains false, with a waiver recorded in `config/release-policy.json`.

Only a fresh allowlisted source tree is eligible for GitHub: original code, public documentation, provenance/notice files and workflows. No prior local Git history, `.env`, device disks, accounts, chats, attachments, private audit logs, installers or test fixture state are included. Original code is Apache-2.0; bundled third-party source retains its own notices. Remaining binary notice gaps block uploading installer artifacts in this batch. The public Issue forms request reproducible environment details and sanitized logs.

Product acceptance remains distinct: language/platform completeness, long-running host recovery and remaining redistribution review are not declared complete by publishing source. `package --public` continues to reject an incomplete full-product release.

## Checks in this batch

- 105 Bun tests / 1,747 assertions passed, including channel/network and script regressions.
- 48 Python tests passed with 11 explicit fixture/platform skips; these skips are not new integration passes. Five source-export tests distinguish source authorization from product acceptance.
- Four TypeScript projects, Next production build, native strict Clippy and 12 CSS parses passed.
- The source allowlist contains 625 files before generated contributor instructions/manifest. Exact private-credential comparison found no matches. README/index links resolve and Git ignore excludes private state, build products and signing artifacts while retaining `.env.example`.

## Online source

Published to [Conferringo/AI-Phone](https://github.com/Conferringo/AI-Phone) as an experimental source preview. The first remote tree was checked against every local Git blob (627 files including contributor instructions and the source manifest). Issue forms and private vulnerability reporting are enabled. Read-only source/native checks were triggered; their current results are visible in [Actions](https://github.com/Conferringo/AI-Phone/actions). A running workflow is not a completed check. No installer Release or user state was uploaded.

The first Android CI run exposed `sdkmanager: command not found` before compilation. The workflow now calls the SDK manager by its explicit runner SDK path and exports Java/SDK variables for the current step and later steps, following the [runner image setup](https://github.com/actions/runner-images/blob/main/images/ubuntu/scripts/build/install-android-sdk.sh). This fix is included in the follow-up commit; rerun results remain distinct from physical device tests.
