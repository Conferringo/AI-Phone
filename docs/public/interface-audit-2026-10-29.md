# Interface maturity audit / 接口成熟度审计

Date: 2026-10-29. Scope: model configuration → request/stream → tool result; channel ownership/delivery; client versus host platforms; current GitHub release documentation. This is not a whole-repository SOLID certification or a new binary release.

## Findings / 发现

| ID | Evidence and burden / 证据与影响 | Result / 本批结果 |
| --- | --- | --- |
| I01 | Responses and Chat Completions each owned newline buffers, byte limits and `data:` parsing. Both rejected valid multiline SSE; neither respected the blank-line event boundary. Production consumers are worker requests and configuration probes. | Replaced both framers with `model_sse::SseDecoder`; completion/tool policies remain protocol-specific. Multiline regressions failed before the change and pass after it. |
| I02 | Transport appended a suffix unconditionally. Supplying a complete `/v1/responses` address produced `/v1/responses/responses` and HTTP 404. | `ModelConfig::endpoint` owns suffix selection. Complete matching URLs are used once; version prefixes remain; mismatched protocols are rejected. The normalized stored configuration and key-retention rules are unchanged. |
| I03 | HTTP 200 with an invalid response MIME type could still produce model success. | Requests advertise SSE; successful responses must identify `text/event-stream`. The loopback HTTP regression failed before the change. Provider error bodies are still bounded/redacted and cancellation remains active. |
| I04 | The Chat translator ignores reasoning items/content; DeepSeek requires genuine `reasoning_content` on tool continuation and constrains forced choices in thinking mode. | **Open, P1.** The three configuration probes do not certify every thinking mode. Do not transplant/fabricate reasoning or mark native thinking compatibility complete. |
| I05 | Four channel adapters exist, but account-level incoming/outgoing/image/notification checks are not all complete. Incoming attachments are absent. | **Open, P1.** Adapter/unit evidence is separated from real-account acceptance in the compatibility table. No new unconnected platform buttons were added. |
| I06 | Five client installer artifacts were easily confused with four complete local hosts. | **Open, P0.** Published clients still need a server. Windows/Linux host adapters and Android independent hosting remain unfinished. Native setup work is not a new validated installer release. |
| I07 | The current release page combined stale claims of no Release, unrun native CI and remaining notice gaps, despite a prerelease with 21 assets and successful release CI. | Replaced the contradictory diary summary with one release snapshot, capability matrix, prioritized exit criteria and links to dated historical evidence. |

DeepSeek references: [thinking mode](https://api-docs.deepseek.com/guides/thinking_mode/), [tool-choice constraints](https://api-docs.deepseek.com/api/create-chat-completion/). The official `deepseek-flash` model ID remains valid; lack of proper thinking/tool policy is a separate problem from the preset name.

## Contract and operation receipt / 契约与变更记录

- **Boundary/owner:** model SSE framing only moved to the shared decoder. Configuration owns URL resolution. The execution engine still owns actual actions, observation freshness, cancellation and unknown-result policy.
- **Retired obligation:** two independent event-framing/size-limit implementations become one. No dependency, supported protocol, protobuf field, endpoint, stored version or channel is retired. The protocol parsers remain because their terminal/tool rules differ; merging them was rejected.
- **Reachability:** the worker and model capability probes call the same transport. The local HTTP tests exercise its real requests/responses. Arbitrary external gateway behavior and live vendor compatibility remain externally unverified.
- **Compatibility:** SSE events now require a blank line as specified by the standard. EOF never turns partial content into tools. A nonconforming gateway may now fail explicitly; it must correct its framing/content type. Tests that previously omitted event delimiters were corrected while preserving their original assertions.
- **Persistence:** no migration, credential rewrite, device/control request or chat mutation. Existing revision fingerprints and saved-key restrictions remain in place.
- **Verification:** assistant unit/loopback tests and strict Clippy; SSE boundary, multiline, Unicode, BOM, resource-limit, completion, truncation, tool-ID and cancellation cases. Real provider/DB/process tests retain explicit ignored status; this batch makes no live model call.
- **Maintainability checks:** crapper reports shared decoder functions at CC 8 and 7, with coverage/CRAP N/A. Other transport functions remain above CC 10; no whole-module cleanliness claim. Dryer found no ≥0.8 duplicate candidate in the three inspected model files. The decoder mutation run detected all 11 generated changes, with zero survivors. This is distinct from line coverage and is not extrapolated to the product.
- **Undo:** revert the scoped model/parser/configuration changes and their tests; remove the module declaration with the new file. No user data or config restoration is necessary. Historical status statements remain in dated reports and Git history.

## Next review / 下一步

1. Fix I04 as its own provider-policy change with a strict multi-turn fixture. Preserve genuine model context and engine-generated tool provenance; forced observation and thinking must both work.
2. Certify channels with real accounts and separate text, image, notification and unknown delivery receipts. Never turn an unknown send into a blind retry.
3. Finish client-managed first run per platform; do not conflate a build, an install waiver and runtime acceptance.
4. Continue the existing UI/execution ownership audit. [Current release status](release-status.md) owns priority; [compatibility](compatibility.md) owns protocol/platform scope; [SOLID audit](solid-audit.md) retains broader code debt.
