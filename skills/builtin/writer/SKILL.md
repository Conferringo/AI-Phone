---
name: writer
description: 真实Linux LibreOffice Writer的查找、焦点核对与退出查找；仅内置已实测流程。
---

通过computer-use操作application_id=linux:libreoffice-writer。先观察真实窗口；文件名和标题不是应用身份。这里的查找流程已实测，不代表排版、替换、导出或保存等未在本轮验证的能力。

Ctrl+F打开查找栏。重填关键词前核对输入焦点和已有文本：再次Ctrl+F可让查找栏获得焦点并选中旧关键词；已有选中态时直接computer_type替换即可。若Ctrl+A选中了正文整篇，停止输入，重新聚焦查找框，避免把查找任务变成覆盖文档。每步使用动作返回的新画面，不重复请求同一截图。

输入关键词后按Return。以正文实际选区和状态栏匹配信息核对命中；“Reached the end of the document”只说明搜索到文末，单独不能证明命中或未命中。查找栏有焦点时Escape可关闭它；若焦点在正文，先重新聚焦或使用当前可见关闭按钮，再确认查找栏已消失。

只读查找不做保存、替换或关闭文档。既有修改状态由用户持有，不能为了验收覆盖或丢弃原内容。小字可用computer_zoom，但裁剪不是新观察，动作仍使用完整画面的坐标和当前观察ID。

可复用心得使用app_experience_save，app_id取真实application_id，引用本轮完整观察ID。仅记通用步骤、焦点恢复和已验证边界，不记录私人文件名、查询内容、正文或凭据。未知版本仍以真实控件为准；不能把经验或Skill阅读当作执行证据。
