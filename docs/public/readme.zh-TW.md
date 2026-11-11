# AImanity

[English](../../README.md) · [全部語言](../../README.md) · Apache-2.0

## 下載與現況

公開下載待發布。 [GitHub Releases](https://github.com/MrMilkDragon/AImanity/releases)

支援 Apple Silicon macOS 14.4 以上與 Windows 11 x64。安裝包內建 Android、Linux、模擬器及本機服務。

## 功能與情境

AImanity 將真實 Android 客體、輕量 Linux 桌面及 Agent 放在同一工作台。預設一部手機、一台電腦，各自保留連續對話，可共用 Agent 或分別指定模型。

- 操作微信等應用：觀察真實畫面、檢視通知，依使用者指示回覆；帳號及應用限制仍然有效。
- 跨裝置工作：手機收件，傳到 Linux 處理、核對試算表，再交回手機。裝置間傳輸成功與交付聯絡人是不同結果，需分別驗證。
- 持續任務：可持續補充指示和中斷生成，先前訊息與操作紀錄保持；通知可喚醒 Agent，Liberty 可逐裝置暫停。
- Skill、關聯知識文件、工作檔案全域共享；模型/API、記憶、外部連接屬於 Agent。Clash 以應用形式存在於客體，宿主預設直連。

有副作用的動作綁定觀察、裝置、任務、控制 epoch 及唯一 action ID；結果未知時先觀察再決定是否重試。

## 架構與資料

`apps/web`：Next.js App Router、React、Tailwind、shadcn；`apps/client`：Tauri 2；`crates`：Rust/Axum/Tokio；`packages/protocol`：protobuf。PostgreSQL/SQLx 保存狀態，Redis 預留為選用快取，非任務佇列。Bun/Cargo 為開發工具，Compose 承載服務。`infra`、`scripts`、`skills`、`config` 存放執行環境和內建能力。

程式碼與使用者狀態分離。裝置磁碟、帳號、聊天、附件、金鑰及私有證據不進入公開儲存庫。更新應使用版本化產物、備份、遷移與回復，不直接在使用者資料上執行 `git pull`。

## 後續方向、責任與授權

優先完成全包部署、中斷恢復、簽署更新、長時間 Agent 穩定性、畫音與輸入、模型能力檢測及應用特化 Skill。語音及實驗性通話屬後續方向。

介面和初次配置支援十種語言。Skill 左側語言入口即時切換介面，阿拉伯語採 RTL；不自動改變客體系統語言、記憶或翻譯工具原文。

使用者須管理權限、帳號、外部操作及第三方條款。不得用於詐騙、欺瞞或未經同意的行為。原創程式碼採 Apache-2.0，第三方元件遵守各自授權。

[發行狀態](release-status.md) · [技術架構](architecture.en.md) · [相容範圍](compatibility.md) · [第三方聲明](../../THIRD_PARTY_NOTICES.md) · [免責說明](../../DISCLAIMER.md)
