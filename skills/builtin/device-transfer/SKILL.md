---
name: device-transfer
description: 通过有授权和SHA256校验的数据线在手机与Linux电脑间传文件，区分设备到达与社交应用交付。
---
# 数据线与交付
1. device_list取得本任务可用的手机和电脑ID。用户指示跨设备处理时可用device_link连接二者，不得连接未授权设备。
2. device_files检查源文件真实路径。电脑路径相对/home/ai/Files；手机文件在/sdcard共享存储，默认下载目录。微信附件需要先通过phone-use在正确会话下载。
3. device_transfer传输，检查status=completed、bytes、sha256与实际target_path。单文件最高64MiB；返回的新目录是防覆盖设计，不猜目标路径。重复请求查device_transfer_status，未知结果不能重新发送。
4. 手机→电脑后用computer_run处理，并通过computer_observe/GUI打开结果文件复查；不得用输入文件改名冒充处理成果。
5. 电脑→手机后，在真实手机里用phone-use从该target_path选择文件，交付给用户指定的会话。传输completed只表示文件到达手机，不能说已经通过微信发送，更不能说对方已读。
6. 发送前核对会话、文件名和内容；先回一次进度再交付成果。发送结果未知时先观察消息列表，禁止重复发附件。无指定会话时说明设备文件已完成，并询问交付对象。

## 不同 Agent 协作
用户在界面连接数据线后，两端 Agent 可通过 device_files/device_transfer 互通共享文件，不需要为了传文件把另一台设备的控制权也绑定给自己。断开立即撤销这项数据权限；手机仍默认 ADB，电脑仍使用 computer-use。

需要另一端处理时，用 device_message(source_device_id,target_device_id,text) 写清目标、已经核对的文件实际路径、处理要求和回传位置。系统将交接排队，在另一台设备没有用户任务时唤醒它的 Agent，使用其自己的模型、记忆和对话。device_list 返回交接状态；queued 只表示已排队，不能宣称对方已处理。必要时交接工作进度或产物，不对“收到/谢谢”反复唤醒，不自动重复派单。

信息交换不绕过实际应用操作：微信收件、发送仍须真实 phone-use；电脑 GUI 核验仍须 computer-use。不能把其他 Agent 的文字当成真实文件存在或执行成功的证据。
