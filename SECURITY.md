# Security and privacy / 安全与隐私

The preview is intended for a trusted local network. Local HTTP does not encrypt traffic. Do not expose the control APIs, ADB or database directly to the Internet. Remote clients require trusted TLS or a user-managed secure network.

Report vulnerabilities through [GitHub private security reporting](https://github.com/Conferringo/AI-Phone/security/advisories/new), enabled for this repository. Do not post credentials, device screenshots, contact lists, conversations or private account data in public issues. Ordinary sanitized bug reports belong in Issues.

Private state includes `.env`, `user/`, `data/`, device disks, model credentials/revisions, channel vaults/keys, attachments and audit evidence. API keys are intentionally visible to the authenticated administrator in configuration; ordinary state APIs must not expose them. Never place keys in NEXT_PUBLIC variables, screenshots or error logs.

Unknown input or delivery outcomes must be observed before retrying. Tests must not steal a user's active device control. Recovery/updates must preserve action deduplication, conversation history and pairing scopes. Guest root does not confer arbitrary host privileges.

The release scanner is a precaution, not a proof of absence of secrets. Checksums establish file integrity, not publisher authenticity. Signed updates, clean-machine recovery and long-duration execution remain release requirements.
