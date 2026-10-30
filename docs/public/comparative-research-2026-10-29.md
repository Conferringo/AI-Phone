# 对照实现研究 / Comparative implementation review

2026-10-29。结论：**尚未“挖透”这些项目**。本轮固定各仓库提交，选择初次配置、模型验证、语音与电话、设备观察执行几个路径读源码；没有运行整套上游产品，也没有将它们的成熟度等同于 AI Phone。实现使用本项目原有技术栈，未复制这些项目的代码或凭据。

| 项目 | 固定提交 | 阅读范围 | 借鉴与保留差异 |
|---|---|---|---|
| openclaw/openclaw | `086c7e884cf48008a5a91c781c13c4231befe3b1` | 自定义模型配置；语音线路接口、呼叫落盘、回调重放键 | 模型发现失败保留手工 ID；协议先探测后保存；通话先持久化、未知状态查询。当前仅落实明确预设/检测，模型发现和电话尚未实现。 |
| NousResearch/hermes-agent | `780d5a3313d7e1f64c0a0f3ec6b644d50f156e90` | 桌面初次布局的保存/恢复；CLI 快速设置；语音转写/播放 | 引导不能覆盖用户已有布局；空流式转写可回退文件转写；录音与可中断播放分开。AI Phone 保留原工作台，仅在配置窗口引导；语音回退是后续任务。 |
| zai-org/Open-AutoGLM | `86f55382982fb054e8fc98ca80609dff8a2cdc3c` | 启动要求与模型探测；agent step；动作分发和归一化坐标 | 每步观察后再规划；模型响应与动作结果分离。AI Phone 保留自己的控件定位/OCR 和动作账本，不用坐标调用替代 epoch/action_id 约束。 |
| openai/codex | `c92700d2993707ee16eb63b35a1cdd7fcc563f69` | TUI onboarding 状态；app-server 会话/账号模型接口文档 | 步骤有隐藏/进行/完成状态；异步账号状态重新读取，不把通知当最终授权。本轮引导根据已保存配置推进；不复制登录或目录信任流程。 |

## 源码定位

- openclaw/openclaw: [src/commands/onboard-custom.ts](https://github.com/openclaw/openclaw/blob/086c7e884cf48008a5a91c781c13c4231befe3b1/src/commands/onboard-custom.ts), [extensions/voice-call/src/manager/outbound.ts](https://github.com/openclaw/openclaw/blob/086c7e884cf48008a5a91c781c13c4231befe3b1/extensions/voice-call/src/manager/outbound.ts), [extensions/voice-call/src/manager/replay-keys.ts](https://github.com/openclaw/openclaw/blob/086c7e884cf48008a5a91c781c13c4231befe3b1/extensions/voice-call/src/manager/replay-keys.ts), [extensions/voice-call/src/providers/base.ts](https://github.com/openclaw/openclaw/blob/086c7e884cf48008a5a91c781c13c4231befe3b1/extensions/voice-call/src/providers/base.ts).
- NousResearch/hermes-agent: [apps/desktop/src/components/onboarding/first-run/assembly.ts](https://github.com/NousResearch/hermes-agent/blob/780d5a3313d7e1f64c0a0f3ec6b644d50f156e90/apps/desktop/src/components/onboarding/first-run/assembly.ts), [hermes_cli/setup_quick.py](https://github.com/NousResearch/hermes-agent/blob/780d5a3313d7e1f64c0a0f3ec6b644d50f156e90/hermes_cli/setup_quick.py), [tools/voice_mode.py](https://github.com/NousResearch/hermes-agent/blob/780d5a3313d7e1f64c0a0f3ec6b644d50f156e90/tools/voice_mode.py).
- zai-org/Open-AutoGLM: [main.py](https://github.com/zai-org/Open-AutoGLM/blob/86f55382982fb054e8fc98ca80609dff8a2cdc3c/main.py), [phone_agent/agent.py](https://github.com/zai-org/Open-AutoGLM/blob/86f55382982fb054e8fc98ca80609dff8a2cdc3c/phone_agent/agent.py), [phone_agent/actions/handler.py](https://github.com/zai-org/Open-AutoGLM/blob/86f55382982fb054e8fc98ca80609dff8a2cdc3c/phone_agent/actions/handler.py).
- openai/codex: [codex-rs/tui/src/onboarding/onboarding_screen.rs](https://github.com/openai/codex/blob/c92700d2993707ee16eb63b35a1cdd7fcc563f69/codex-rs/tui/src/onboarding/onboarding_screen.rs), [codex-rs/app-server/README.md](https://github.com/openai/codex/blob/c92700d2993707ee16eb63b35a1cdd7fcc563f69/codex-rs/app-server/README.md).

DeepSeek 新表单的默认地址、模型和 Chat Completions 来自本轮核对的[官方首个请求文档](https://api-docs.deepseek.com/)。已有配置不因上游示例变化而被改写。电话层的产品范围另外参照 [OpenClaw Voice Call](https://docs.openclaw.ai/plugins/voice-call)。

## 下一轮要继续研究和实测

1. 模型发现/能力探测的错误分类、缓存失效和服务商 OAuth；现有两个协议适配器覆盖不到所有服务商。
2. 语音会话的打断时序、VAD 与回声、流式断线、工具执行期间的用户补充。以记录回放和故障注入评估，不凭 UI 相似判断能力。
3. 电话重启恢复、回调乱序、费用边界和不可确认拨号。必须先 mock，再内网 SIP，再明确用户授权的实际线路。
4. 持久记忆冲突、技能来源/升级/回退、跨设备任务交接；继续建立可重复场景，不能把模型自称成功当成功证据。
5. 各上游安装器/自动更新的完整故障路径。本轮没有完成端到端运行比较，也没有做量化性能排行榜。

## English

This is a targeted, commit-pinned source review, not an exhaustive comparison or an upstream runtime benchmark. The references above cover setup, provider verification, device observations, voice fallback and durable call handling. AI Phone retains its own protobuf contracts, explicit device bindings and action ledger. Provider discovery, realtime voice and telephony are roadmap items; reading their implementations does not mean those features have shipped here.

No upstream code was copied in this batch. License compatibility still requires review before future code reuse. See the [release roadmap](release-roadmap.en.md) for the staged adoption and acceptance criteria.
