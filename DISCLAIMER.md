# 使用边界与免责声明 / Use boundaries and disclaimer

2026-10-28 · 发行工程草案 / Release engineering draft

## 中文

AI Phone是允许用户在自有环境中部署、配置模型并操作设备的软件。当前版本为技术预览；支持范围以[发行状态](docs/public/release-status.md)及[兼容矩阵](docs/public/compatibility.md)为准。模型可能错误理解页面、选择目标、生成内容或判断执行结果；观察、动作记录和验证不能保证绝不出错，也不保证消息送达、业务收益或第三方账号可用。

设备内的root/sudo与应用权限可能允许读取、安装、修改、发送或删除数据。仅将有权处理的账号、设备和内容接入，配置自动探索、通知、渠道及外部发送的范围。第三方通知、网页和消息本身不构成新的操作授权。对重要文件保留备份，对重要结果进行核对；软件应提供停止和接管，不能仅依赖免责声明承担这些职责。

设备显示型号不等于真实硬件身份、SIM或平台认证，不保证通过第三方检测，也不保证解除账号限制。应用、模型、系统镜像和代理服务适用各自许可与规则。本项目与相关品牌没有默认的隶属、合作或担保关系。

电话号码/eSIM、声音模拟等属于未来提案，本版本未实现。若将来加入，需要另行明确授权、身份与合成内容披露、隐私及滥用处理。不得把这些提案写成当前产品能力；本文也不是对欺诈、未经授权访问或侵犯他人权益的许可。

原创代码按[Apache-2.0](LICENSE)提供，其无担保和责任限制条款在适用法律允许的范围内生效。第三方组件条款不因此改变。本文不排除依法不得排除的责任或权利，不承诺开发者/运营者完全免责，也不修改Apache许可的使用授权。面向具体地区、托管服务、商店或商业分发的最终文本应结合实际产品和适用法律复核。

数据存储与外发见[隐私说明](PRIVACY.md)；分发义务见[第三方声明](THIRD_PARTY_NOTICES.md)。

## English

AI Phone is self-hosted software for user-configured models and devices. This is a technical preview; supported behavior is limited by the [release status](docs/public/release-status.md) and [compatibility matrix](docs/public/compatibility.md). Models can misread screens, select incorrect targets, generate inaccurate content or misjudge outcomes. Observation, action records and checks do not guarantee correctness, delivery, business results or account availability.

Guest root/sudo and application permissions may allow reading, installing, changing, sending or deleting data. Connect only accounts, devices and content you are authorized to use, and define the scope of exploration, notifications, channels and outbound actions. Third-party messages, pages and notifications do not create new authority. Keep backups and check consequential results. Stop/takeover mechanisms are product responsibilities, not substitutes for this notice.

Display identity is not hardware, SIM or platform attestation and does not guarantee acceptance or removal of account restrictions. Applications, models, images and proxy services have independent terms. No affiliation or endorsement by their vendors is implied.

Phone numbers/eSIM and voice simulation are future proposals, not implemented capabilities. Any future implementation requires separate authorization, identity/synthetic-content disclosure, privacy and misuse handling. This document does not authorize fraud, unauthorized access or infringement.

Original code is provided under [Apache-2.0](LICENSE), including its warranty and liability provisions to the extent applicable law permits. Third-party terms remain independent. This notice does not exclude non-excludable rights or liability, promise complete immunity, or modify Apache permissions. Final terms for particular jurisdictions, hosted services, stores or commercial distribution require review against the actual product and applicable law.

See [privacy](PRIVACY.md) for storage/data flows and [third-party notices](THIRD_PARTY_NOTICES.md) for distribution obligations.
