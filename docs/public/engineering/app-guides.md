# Application-specific operation guides

AI Phone combines a small shipped guide with the user's evidence-backed experience for the current application. Both are references for real GUI execution, not trained model weights or proof that an application is fully supported.

## Runtime path

`builtin_guides.rs` is the single registry for shipped Skill installation and exact machine-application bindings. A phone observation supplies its Android package; a desktop observation supplies its separate OS application ID. Private window titles and quoted app names do not select a guide.

The existing worker queues the matching built-in `skill_read` before loading enabled personal experience and before requesting another model decision. It uses the existing checkpoint, tools and Skill resource owner. Remaining in the same application does not repeat the reads or database lookup. An unavailable optional guide is attempted once in the current input and does not hide local experience. Unknown observations cannot borrow an earlier application's identity.

Shipped guides do not replace the user's learned Skill, manual notes or knowledge documents. Candidate publication still requires the current task's real observations and completion verification. Corrections can revise a method; complete long-term merging, effectiveness measurement and retirement are still unfinished.

## Qualified scope

| Application | Shipped guidance and evidence | Limits |
| --- | --- | --- |
| WeChat | Existing notification, conversation and reply guidance | The complete real incoming work request → GUI office work → original-chat delivery scenario remains unqualified. |
| Xiaohongshu | Android 9.51.0: GUI search, open a result, inspect its visible title, dismiss an optional login sheet and return to results | Read-only flow only. Full articles/comments behind login, publishing, private messages and commerce are not qualified. No fixed coordinates or personal content are shipped. |
| LibreOffice Writer | Real GUI find, input-focus recovery, result verification and closing the find bar; guided experience followed by automatic reuse | Does not certify document editing, replacement, formatting or export. |
| Douyin, QQ, DingTalk, WeCom, Meituan, bilibili | General phone-use tools are available | Per-application scenarios have not been individually qualified. There are no placeholder guides claiming otherwise. |

Guest applications, versions, language and account state can change the UI. Controls and screenshots from the current observation determine the next action. A visible title does not establish reading an entire article; a queued or locally visible message does not establish recipient receipt.

## Adding a qualified guide

1. Perform the intended scenario through real phone-use or computer-use, including the relevant blocked and recovery paths. Keep private evidence outside public source.
2. Write reusable control/focus rules and observed limits in `skills/builtin/<id>/SKILL.md`. Omit accounts, contacts, private file names, queries, article contents and credentials.
3. Add the exact observed application binding to the existing registry. The installer and worker consume that one registry.
4. Exercise fresh installation, built-in plus local experience, optional failure, checkpoint reuse and actual device execution. Compilation or a written guide alone is not application qualification.

## 中文

内置专项指南和用户自己的经验分别保留，按真实观察到的包名或电脑应用标识加载，沿用同一工具与检查点，不新增学习Agent。当前仅小红书只读搜索/查看标题/返回、Writer查找与焦点恢复有对应的真实演练；微信完整办公交付及其他App仍未逐项通过。指南不能代替当前画面、真实操作和完成核验，也不能将经验积累称为模型权重训练。
