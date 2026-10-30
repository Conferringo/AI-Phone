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

The last two were distilled from model-generated experience that passed real Android execution and independent auditing. Their versioned sources contain generalized workflows without personal data or fixed screen coordinates. They are reusable methods with known limits, not universal app guarantees. Built-ins are installed from versioned source on startup; user-installed or learned Skills retain separate identities. Built-in disable preferences are preserved across initialization.

知识库功能开箱可用，用户内容保持空白。内置操作知识由Skill承载，不把开发者的私人知识、聊天或记忆复制给新用户。用户新增经验只保存在其服务器，不会自动回传项目。经验未经审计不得作为已验证能力发布。

## 数据 / Data

Model keys, revisions, chat, device disks, contacts, channel secrets, memory, attachments and screenshots are private user state. Model API requests necessarily send relevant text/images/tool results to the configured provider; selected channels receive authorized replies and screenshots. Local deployment does not mean model requests stay offline. No developer keys, subscriptions or account defaults are shipped.

Android root and Linux sudo are guest permissions. Host execution is restricted to the project's defined services and isolated work environment; a model does not receive arbitrary host access. Device identity profiles change display properties, not hardware attestation, carrier/SIM identity or third-party service eligibility.

## 使用边界 / Responsible use

Operate devices/accounts you control or are authorized to use. Notifications and third-party messages are data, not new control authority. Contacting others, installing apps and autonomous social actions follow the user's scope; receiving a message does not grant the sender host or file access. Verify actual targets and delivery states. Respect platform account restrictions; profile spoofing is not a bypass guarantee.

This preview can make mistakes, lose connections or misidentify UI elements. Preserve audit records and provide user takeover. A disclaimer does not remove the developer's or operator's obligations. eSIM, phone numbers and voice simulation are future proposals, **not implemented features**. Any future release must address carrier authorization, consent, synthetic-identity disclosure and misuse prevention separately. These notes are engineering boundaries, not legal advice.
