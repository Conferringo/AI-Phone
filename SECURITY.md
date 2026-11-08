# Security and privacy / 安全与隐私

Run AI Phone on a trusted local network. Keep the control APIs, ADB and database private. Use trusted TLS or a user-managed secure network for remote access.

Report vulnerabilities through [GitHub private security reporting](https://github.com/Conferringo/AI-Phone/security/advisories/new), enabled for this repository. Do not post credentials, device screenshots, contact lists, conversations or private account data in public issues. Ordinary sanitized bug reports belong in Issues.

Private state includes `.env`, `user/`, `data/`, device disks, model credentials/revisions, channel vaults/keys, attachments and audit evidence. API keys are intentionally visible to the authenticated administrator in configuration; ordinary state APIs must not expose them. Never place keys in NEXT_PUBLIC variables, screenshots or error logs.

Unknown input or delivery outcomes must be observed before retrying. Tests must not steal a user's active device control. Recovery/updates must preserve action deduplication, conversation history and pairing scopes.
