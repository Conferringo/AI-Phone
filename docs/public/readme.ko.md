# AImanity

[English](../../README.md) · [전체 언어](../../README.md) · Apache-2.0

## 다운로드와 현황

공개 다운로드는 준비 중입니다. [GitHub Releases](https://github.com/Conferringo/AI-Phone/releases)

Apple Silicon이 탑재된 macOS 14.4 이상 또는 Windows 11 x64가 필요합니다. 각 설치 파일에 Android, Linux, 에뮬레이터와 로컬 서비스가 들어 있습니다.

## 기능과 사용 시나리오

실제 Android 게스트, 가벼운 Linux 데스크톱과 Agent를 한 작업 공간에서 사용합니다. 기본 구성은 휴대폰 한 대와 컴퓨터 한 대입니다. 기기별 대화가 유지되며 Agent를 공유하거나 다른 모델을 지정할 수 있습니다.

- WeChat 같은 앱의 실제 화면과 알림을 확인하고 사용자 지시에 따라 조작하고 답장합니다. 계정과 앱의 제한은 그대로 적용됩니다.
- 휴대폰에서 받은 파일을 Linux로 전송하고 표를 처리·검토하여 돌려보냅니다. 기기 간 전송과 연락처 전달은 별도로 검증합니다.
- 지시를 추가하고 생성을 중단해도 기존 메시지와 작업 기록은 보존합니다. 알림으로 Agent를 깨우며 Liberty는 기기별로 일시정지합니다.
- Skill, 연결된 지식 문서와 작업 파일은 공유합니다. 모델/API, 기억과 외부 연결은 Agent에 속합니다. Clash는 각 게스트 안의 앱으로 실행되고 호스트는 기본 직접 연결을 사용합니다.

작업은 관찰, 기기, 실행, 제어 epoch와 고유 ID에 연결됩니다. 결과가 불명확하면 재실행 전에 관찰합니다.

## 기술과 데이터

`apps/web`: Next.js App Router, React, Tailwind, shadcn. `apps/client`: Tauri 2. `crates`: Rust/Axum/Tokio. `packages/protocol`: protobuf. PostgreSQL/SQLx는 상태를 저장하고 Redis는 선택적 캐시용으로 예약되어 있으며 작업 큐가 아닙니다. Bun/Cargo로 개발하고 Compose로 서비스를 실행합니다. `infra`, `scripts`, `skills`, `config`에 실행 환경과 내장 기능이 있습니다.

코드와 사용자 상태는 분리됩니다. 기기 디스크, 계정, 대화, 첨부와 키는 공개하지 않습니다. 업데이트는 버전별 배포물, 백업, 마이그레이션과 롤백을 사용해야 하며 사용자 데이터 위에서 직접 `git pull`하지 않습니다.

## 향후 방향·책임·라이선스

완전한 패키지, 설치 재개, 서명된 업데이트, 장기 Agent 안정성, 음성/입력, 모델 검증과 앱별 Skill을 우선합니다. 음성 대화와 실험적 전화 기능은 이후 방향입니다.

인터페이스와 초기 설정은 10개 언어를 지원합니다. Skill 왼쪽의 언어 버튼으로 전환하며 게스트 OS와 도구 원문은 자동 번역하지 않습니다. 아랍어는 오른쪽에서 왼쪽으로 표시됩니다.

사용자는 권한, 계정, 외부 작업과 앱 약관에 책임을 집니다. 사기, 기만 또는 동의 없는 작업에 사용하지 마세요. 자체 코드는 Apache-2.0이며 타사 구성 요소는 각 라이선스를 따릅니다.

[출시 현황](release-status.md) · [아키텍처](architecture.en.md) · [호환성](compatibility.md) · [타사 고지](../../THIRD_PARTY_NOTICES.md) · [면책 안내](../../DISCLAIMER.md)
