# UI system / 界面约定

Canonical tokens live in `apps/web/src/styles/tokens.css`; the workbench layout is `apps/web/src/app/phone/device-workbench.css`. Use the existing neutral glass material, one Lucide icon family and visible focus states. Shared tools expand from their actual anchors. Switching tools closes the previous window and its children while preserving drafts. Short labels, real errors and real connected functions take precedence over explanatory copy.

One phone and one computer occupy the same perspective stage; clicking an exposed device brings it forward. A cable connection creates a paired layout. In portrait phone view, the shared conversation sits to the right. In computer, landscape and paired views, it sits below the devices. Device height follows the window and short windows can scroll to keep the input accessible.

Device name, volume, mute, rotation (phone only) and screenshot sit above each screen. The cable control sits in the shared toolbar. Screenshot previews support saving or attaching to the shared chat. Skills, knowledge and work files are shared. Model settings, memory and application connections stay with each saved Agent.

GSAP layout movement, inner device perspective and conversation translation have separate property owners. Capture only real layout/orientation changes; an unchanged observation must not finish the current animation. Preserve mounted streams, drafts and native cursors through transitions. Support reversal, cleanup, hidden pages and reduced motion.

Eight catalogs share stable keys and interpolation. Never translate user chat, document content, device names or credentials. Review actual English/Chinese input, save/error paths, tool switching, narrow layouts and reduced motion; a language selector alone is not translation acceptance. Keep screenshots containing credentials, subscription URLs or pairing secrets out of public evidence.

Startup opens the device workbench directly; no forest, welcome action or logout control remains in personal mode. Session failures show an actionable retry. The knowledge motion renderer is original AI Phone code with source hashes in config/source-assets.json; its complete visual acceptance remains pending. EPIC is a visual reference, with no copied renderer in the source package; unrelated product data and credentials are never included.

All controls use the shared 8px radius, compact items 6px and windows/popovers 14px. Circle indicators and device hardware frames retain their semantic shapes. Entry/setup/recovery surfaces share entry.css; floating windows share window.css. Native setup handles language and device resources. First-run Agent setup opens the model form directly and checks the devices from the model list. Connections and memory stay in Agent settings.


## 2026-10-30 control consolidation

The language glyph immediately left of Skills is the only workbench language entry. It opens in place on hover, click or Arrow Down, and Escape dismisses it. Native first-run language selection remains available before the workbench exists.

Liberty has one toggle in the shared conversation. Notifications, replies, installation and application learning use enabled defaults; explicit pauses and failure stops persist.

The conversation morphs its measured position and dimensions using GSAP Flip. The layout hook handles hidden pages, interruption and reduced motion. Conversation history and drafts follow the shared workbench.

Knowledge keeps its graph, search, text documents and document relations. There is no upload/import, recently-deleted browser, view toggle or eye preview. Saved documents show their content; Edit enters the editor and Save commits it. Archived storage rows remain in private backups; their recovery interface is retired.

Route editing accepts freehand pointer strokes, click/keyboard waypoints and undo. Drawing suspends map panning and previews the stroke. Release simplifies corners/endpoints to the 200-point protocol limit; cancel discards only the in-progress stroke.
