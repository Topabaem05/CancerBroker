# 한국어 플러그인 가이드

- [Back to Home](../README.md)
- [Language Index](index.md)
- [Back to Korean README](korean.md)

## OpenCode Session Memory Sidebar Plugin

- 사이드바 플러그인 목적:
  - 현재 세션 + 현재 열려있는 live 세션들의 메모리 상태를 `Session Memory` 패널로 표시합니다.
  - 토큰/컨텍스트 사용량과 RAM 상태를 함께 보여주며, 공유 프로세스 등 정확한 귀속이 불가능한 경우 숫자 대신 `unavailable` 상태를 표시합니다.

- 가장 쉬운 설치 방법 (git clone 불필요):

```bash
curl -fsSL https://raw.githubusercontent.com/Topabaem05/CancerBroker/main/install/opencode-session-memory-sidebar.sh | sh
opencode --restart
```

- 이 bootstrap script는 GitHub Releases의 최신 installer asset을 내려받습니다.

- 요구 사항:
  - `node` 또는 `bun`이 설치되어 있어야 합니다.

- 스크립트를 먼저 내려받아 확인한 뒤 실행하고 싶다면:

```bash
curl -fsSL -o /tmp/opencode-session-memory-sidebar.sh https://raw.githubusercontent.com/Topabaem05/CancerBroker/main/install/opencode-session-memory-sidebar.sh
sh /tmp/opencode-session-memory-sidebar.sh
opencode --restart
```

- 이 방식은 저장소를 clone하지 않고, GitHub에 올라간 standalone installer를 받아 바로 설정만 반영합니다.

- raw 다운로드가 실패하는 환경에서는 인증된 GitHub CLI fallback도 사용할 수 있습니다:

```bash
gh api "repos/Topabaem05/CancerBroker/contents/install/opencode-session-memory-sidebar.sh?ref=main" --jq .content \
  | tr -d '\n' \
  | node -e 'let data=""; process.stdin.setEncoding("utf8"); process.stdin.on("data", (chunk) => data += chunk); process.stdin.on("end", () => process.stdout.write(Buffer.from(data, "base64")));' \
  | sh
```

- Homebrew로 설치할 수도 있습니다:

```bash
brew install topabaem05/cancerbroker/opencode-session-memory-sidebar-installer
```

- 저장소 이름이 `homebrew-cancerbroker`가 아니라서 explicit tap URL이 필요한 환경이라면:

```bash
brew tap topabaem05/cancerbroker https://github.com/Topabaem05/CancerBroker
brew install topabaem05/cancerbroker/opencode-session-memory-sidebar-installer
```

- Homebrew 제거:

```bash
brew uninstall opencode-session-memory-sidebar-installer
```

- 현재 버전 고정 release asset URL:

```text
https://github.com/Topabaem05/CancerBroker/releases/download/CancerBroker-v0.1.0/CancerBroker.cjs
```

- npm publish 이후 지원할 예정인 패키지 실행 방식:

```bash
bunx opencode-session-memory-sidebar-installer
npx --yes opencode-session-memory-sidebar-installer
```

- npm scope를 붙여 배포할 경우:

```bash
curl -fsSL https://raw.githubusercontent.com/Topabaem05/CancerBroker/main/install/opencode-session-memory-sidebar.sh | sh -s -- --package @your-scope/opencode-session-memory-sidebar
```

- 위 명령이 하는 일:
  - 전역 OpenCode 설정 파일 `~/.config/opencode/opencode.json`에 npm 플러그인 `opencode-session-memory-sidebar`를 자동으로 추가합니다.
  - OpenCode는 다음 시작 시 해당 npm 플러그인을 자동 설치/캐시합니다.
  - 이미 추가되어 있으면 중복 추가하지 않습니다.

- 프로젝트 로컬로만 추가하고 싶을 때:

```bash
curl -fsSL https://raw.githubusercontent.com/Topabaem05/CancerBroker/main/install/opencode-session-memory-sidebar.sh | sh -s -- --project
```

- 이 저장소에서 로컬 개발용으로 더 편하게 추가/제거할 때:

```bash
./session-memory-plugin add
./session-memory-plugin add --project
./session-memory-plugin remove
```

- 현재 셸에서 bare command로 쓰고 싶을 때:

```bash
. ./scripts/dev-env.sh
session-memory-plugin add
session-memory-plugin remove
```

- `remove`는 파일 삭제가 아니라 `opencode.json`에서 플러그인 등록만 해제합니다.
- OpenCode도 같이 재시작하고 싶으면 `--restart`를 붙일 수 있습니다.

- 제거:

```bash
curl -fsSL https://raw.githubusercontent.com/Topabaem05/CancerBroker/main/install/opencode-session-memory-sidebar.sh | sh -s -- uninstall
opencode --restart
```

- 배포 구조:
  - npm 플러그인 패키지 소스: `packaging/npm/opencode-session-memory-sidebar`
  - 자동 등록용 installer 패키지 소스: `packaging/npm/opencode-session-memory-sidebar-installer`
  - npm publish workflow: `.github/workflows/npm-publish.yml`

- 배포 원칙:
  - 실제 플러그인 패키지 `opencode-session-memory-sidebar`를 먼저 publish 합니다.
  - 그 다음 installer 패키지 `opencode-session-memory-sidebar-installer`를 publish 합니다.
  - 둘은 버전을 독립적으로 올릴 수 있지만, installer 기본 대상 패키지명은 실제 publish 이름과 항상 맞춰야 합니다.
  - Homebrew 경로도 public 저장소 기준으로 사용할 수 있습니다.
  - release asset은 `.github/workflows/release-installer-asset.yml`과 installer tag로 관리합니다.

- 다음 installer release 준비 자동화:

```bash
node ./scripts/prepare-installer-release.mjs 0.1.1
```

- 이 명령은 installer 버전, standalone asset, Homebrew formula `sha256`, 문서의 versioned release URL을 한 번에 갱신합니다.

- 로컬 개발용 설치 위치 (수동 개발/디버깅):

```bash
ls ~/.config/opencode/plugins/opencode-session-memory-sidebar.ts
ls ~/.config/opencode/plugins/opencode-session-memory-sidebar
```

- 패키지 검증:

```bash
cd packaging/npm/opencode-session-memory-sidebar
bun install
bunx tsc --noEmit -p tsconfig.json
```

```bash
cd packaging/npm/opencode-session-memory-sidebar-installer
bun install
node ./bin/install.mjs --config /tmp/opencode-session-memory-sidebar-test.json
node ./bin/uninstall.mjs --config /tmp/opencode-session-memory-sidebar-test.json
../../../../session-memory-plugin add --config /tmp/opencode-session-memory-sidebar-test.json
../../../../session-memory-plugin remove --config /tmp/opencode-session-memory-sidebar-test.json
```

```bash
cd /Users/guribbong/code/cancerbroker
. ./scripts/dev-env.sh
session-memory-plugin add --config /tmp/opencode-session-memory-sidebar-test.json
session-memory-plugin remove --config /tmp/opencode-session-memory-sidebar-test.json
```

- 기존 플러그인 런타임 테스트:

```bash
cd ~/.config/opencode/plugins/opencode-session-memory-sidebar
bun install
bun test
```

- OpenCode 재시작:

```bash
opencode --restart
```

- 화면에서 확인할 항목:
  - 패널 제목: `Session Memory`
  - 요약 항목: `Live`, `Exact RAM`, `Exact Total`, `Unavailable`
  - 세션 행: `Current <session-id>`, `Other <session-id>`

- 참고:
  - 폴링 주기는 5초(`5000ms`)입니다.
  - exact RAM 합계는 `mappingState=exact` 행만 합산합니다.
  - installer는 JSONC(`//` 주석, trailing comma 포함) 설정 파일도 안전하게 수정하도록 설계했습니다.
  - 검증 로그/증적은 이 저장소의 `.sisyphus/evidence/` 아래에 기록되어 있습니다.
