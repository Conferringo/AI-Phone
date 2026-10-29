# Security and privacy / 安全与隐私

The preview is intended for a trusted local network. Local HTTP does not encrypt traffic. Do not expose the control APIs, ADB or database directly to the Internet. Remote clients require trusted TLS or a user-managed secure network.

Report security issues privately to the project owner. A public vulnerability reporting address has not yet been established; do not post credentials, device screenshots, contact lists, conversations or reproducible private account data in a public issue. Once a public repository exists, configure its private security reporting before release.

Private state includes `.env`, `user/`, `data/`, device disks, model credentials/revisions, channel vaults/keys, attachments and audit evidence. API keys are intentionally visible to the authenticated administrator in configuration; ordinary state APIs must not expose them. Never place keys in NEXT_PUBLIC variables, screenshots or error logs.

Unknown input or delivery outcomes must be observed before retrying. Tests must not steal a user's active device control. Recovery/updates must preserve action deduplication, conversation history and pairing scopes. Guest root does not confer arbitrary host privileges.

The release scanner is a precaution, not a proof of absence of secrets. Checksums establish file integrity, not publisher authenticity. Signed updates, clean-machine recovery and long-duration execution remain release requirements.
