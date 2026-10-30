---
name: files-android
description: 操作 Android DocumentsUI，查找下载文件、识别文件类型筛选范围并核对手机与电脑传输结果。
metadata:
  version: "0.1.0"
---

# Android Files / 手机文件

Open the actual DocumentsUI screen through phone-use. It may restore a previous directory rather than Downloads. Check the current title, breadcrumb and visible file list before selecting a file. Names, modified times and sizes help distinguish duplicate names; do not infer which file is newest from list order alone.

顶部文档/图片/音频/视频芯片可以筛选文件。优先本次原生目标，再用OCR/截图。芯片选中后，其文字可能从UI树消失；取消时根据当前截图识别同一芯片，不沿用旧截图坐标。筛选标题与面包屑可能不一致，不能默认筛选范围就是当前子目录，也不能直接声称全局搜索。

Selecting a spreadsheet without a compatible Android viewer may open an app-store or handler screen. This is not a corrupt-file diagnosis. If the user asked for computer processing and the cable is connected, use device-transfer and computer-use; verify the transfer's device, direction and hash before opening the file on Linux. Disconnects must not be bypassed using a host copy.

After processing, identify the returned file by the confirmed transfer result. Selecting an attachment, sending it, and recipient delivery are separate states. Verify the actual conversation and outgoing attachment before reporting handoff; do not claim the recipient has read it.

来源：经真实 DocumentsUI 文件筛选操作与独立审计支持，保留范围不确定性，去除个人文件名、目录、运行编号与固定坐标。当前版本的列表筛选行为需现场复核。
