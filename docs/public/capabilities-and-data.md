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

知识库初始为空。操作知识由内置 Skill 提供，用户积累的经验保存在自己的服务器。

## 数据 / Data

Model keys, revisions, chat, device disks, contacts, channel secrets, memory, attachments and screenshots are private user state. Model API requests necessarily send relevant text/images/tool results to the configured provider; selected channels receive authorized replies and screenshots. [Data and privacy](../../PRIVACY.md)

Android root and Linux sudo apply inside their respective guests. Host actions use the project services and isolated workspace. Device profiles set the model name and display properties.

## 使用边界 / Responsible use

Use authorized devices and accounts. The account owner sets the scope for incoming content, outgoing messages, app installation and autonomous actions.

The action log shows targets and delivery results. Use the device controls to take over.
