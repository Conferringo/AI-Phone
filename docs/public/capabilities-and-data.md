# Built-in capabilities, private data and boundaries

## 开箱内置能力 / Built-ins

| Skill | Purpose |
| --- | --- |
| phone-use | Observe and act on the real Android guest, native targets/OCR, bounded gestures and result checks |
| computer-use | Screenshot-based Linux interaction, keyboard/pointer and region zoom |
| device-transfer | Linked-device file exchange with direction/path/hash checks |
| wechat | Identify conversations/notifications, authorized replies, reading scope, account restrictions and honest send states |
| autonomous-learning | Evidence-backed app experience, independent audit, revision/withdrawal and avoiding repetitive empty exploration |
| browser-android | Address bar navigation, submit-vs-suggestion distinction, OCR limitations and actual loading checks |
| files-android | DocumentsUI type filters, uncertain scope, missing viewer handling and transfer handoff |

The browser and file Skills came from real Android runs and independent review. Their versioned sources contain reusable workflows with semantic targets. Startup installs built-ins from source and preserves their enabled state. User-installed and learned Skills retain separate identities.

知识库功能开箱可用，用户内容保持空白。内置操作知识由Skill承载，不把开发者的私人知识、聊天或记忆复制给新用户。用户新增经验只保存在其服务器，不会自动回传项目。经验未经审计不得作为已验证能力发布。

## 数据 / Data

Model keys, revisions, chat, device disks, contacts, channel secrets, memory, attachments and screenshots are private user state. Model API requests necessarily send relevant text/images/tool results to the configured provider; selected channels receive authorized replies and screenshots. [Data and privacy](../../PRIVACY.md)

Android root and Linux sudo are guest permissions. Host execution is restricted to the project's defined services and isolated work environment; a model does not receive arbitrary host access. Device identity profiles change display properties, not hardware attestation, carrier/SIM identity or third-party service eligibility.

## 使用边界 / Responsible use

Operate devices/accounts you control or are authorized to use. Use the scope set by the account owner when processing incoming content. Contacting others, installing apps and autonomous social actions follow the user's scope; receiving a message does not grant the sender host or file access. Verify actual targets and delivery states. Respect platform account restrictions.

Use the action log to check targets and delivery, and take over the device when needed. The roadmap covers eSIM, phone numbers and voice simulation. Those extensions require carrier authorization, consent, synthetic-identity disclosure and misuse controls. [Development roadmap](release-roadmap.en.md)
