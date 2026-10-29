# 界面语言 / Interface languages

当前提供简体中文、English、繁體中文、日本語、한국어、Deutsch、Français、Español 八种界面目录。

1. **安装配置**：`./ai-phone configure` 和首次 `./ai-phone install` 的第一项选择语言，输入编号或语言代码；随后资源/代理配置提示切换语言。选择保存在私有部署配置，服务启动使用 `AI_PHONE_DEFAULT_LOCALE`。`configure --profile` 沿用 JSON 的 `locale`，无需交互。
2. **原生客户端**：连接窗口可选语言，连接时传给工作台；限定八个代码，工作台保存cookie后移除导航参数。旧连接配置仍可读取。
3. **首次配置/日常使用**：配置中心的语言选择器即时切换，浏览器刷新保留。优先级：本次客户端明确选择 → 当前浏览器cookie → 安装配置 → 浏览器语言。已有用户cookie不会被普通刷新覆盖。

选择界面语言不会修改 Android/Linux 的系统语言、输入法、Agent记忆或已有聊天；模型输出语言由会话决定。工具结果、操作系统画面、第三方消息和部分后端诊断保留原文，尚未建立全部错误码的八语言映射。八目录键一致不代表全窗口人工翻译/读屏/触屏验收完成。

## English

Eight interface catalogs are available. Choose a language as the first installation-configuration step; numbered choices and locale codes are accepted. The saved private profile supplies the server default. The native connection window can explicitly pass a language to the workbench, which stores a browser cookie and removes the navigation parameter. The configuration center also allows changing the language at any time.

Priority: explicit client choice, browser cookie, installation default, then browser preference. Existing connection files remain compatible. This changes the interface only: guest system languages, input methods, conversations and Agent memories are unaffected. Raw device/tool/channel text and some backend diagnostics retain their original language. Complete translation and native accessibility acceptance remain open release work.
