# Проверка 0.11.1 Alpha

24.10.2026, Linux x86_64, Rust/Cargo 1.99.1, glibc 2.39.

- cargo test --workspace --locked (server/default): 79 тестов, все прошли.
- cargo test --features native-client --locked: 85 тестов, все прошли.
- cargo clippy --features native-client --all-targets --locked -- -D warnings: успешно.
- cargo build --release --features native-client --locked: успешно.
- tests/native_e2e.py: 19 проверок в native Linux X11/WebKitGTK окне; редактирование
  через GUI, Ctrl+S, restart, API token boundary, Home/Quit cleanup, corporate
  connection, сохранение URL без секретов и отказ для небезопасного URL.
- cargo clippy --workspace --all-targets --locked -- -D warnings: успешно.
- cargo build --release --locked: успешно.
- tests/http_smoke.py: 79 проверок.
- tests/workspace_smoke.py: 44 проверки, включая restart, CAS/idempotency,
  роли/org isolation, approvals, publish, offline conflict и backup integrity.
- tests/oidc_smoke.py с FORGEPLAN_RUN_OIDC_BROWSER=1: 29 проверок, включая
  подпись, issuer/audience/expiry, JWKS rotation, роли и браузерный PKCE callback.
- tests/e2e.cjs и tests/workspace_e2e.cjs: успешно, Playwright 1.51.1 / Chromium 134.
  Проверены основной редактор, mobile layout, persistence, modes, personal calendars,
  rates, baseline/EVM/actual, publish, leveling preview/undo и XML import confirmation.
- tests/platform_smoke.py: 29 проверок на Linux x64. OS data directory,
  Unicode/пробелы, restart из другого cwd, private directory permissions,
  occupied port, loopback restriction, safe connect URL, mock browser handler,
  archive checksums, target mismatch/overwrite rejection и распакованный CLI; native launcher дополнительно проверяется window E2E.
- Native JSON schema: пример проверен Python jsonschema; обе схемы читаются как JSON.

Воспроизведение server tests: cargo build --release --locked. Для native tests:
cargo build --release --features native-client --locked, затем xvfb-run/xdo GUI
проверка по NATIVE_CLIENT.md. Остальные Python scripts работают с обеими сборками.
Платформенный тест дополнительно требует Python 3.12 (tar extraction filter).
Браузерные тесты требуют Playwright + Chromium; NODE_PATH и
PLAYWRIGHT_BROWSERS_PATH позволяют использовать отдельную установку.
OIDC smoke требует openssl; FORGEPLAN_RUN_OIDC_BROWSER=1 включает browser test.
Тесты используют синтетические данные и локального тестового IdP.

Не проверены: реальные Keycloak/Entra tenants, PostgreSQL, Docker image execution,
Windows/macOS binaries/installers, MS Project differential XML corpus, MPP,
нагрузочные тесты, security audit, подпись и воспроизводимость release.
Локальный mock IdP не доказывает совместимость со всеми корпоративными IdP.
Эти проверки не означают функционального паритета с Microsoft Project.

Матрица пяти native targets и release recipes валидированы структурно; удалённые
GitHub jobs не запускались. Windows/macOS/Linux ARM64 не объявляются проверенными.

Native GUI test использует Xvfb/xdotool и фиксированное 1440x1000 viewport; это
не полноценный visual/accessibility audit. GTK3/WebKitGTK dependencies загружены
в временный sysroot для этой среды. GUI smoke выполнялся с test-only sandbox/
software-render flags в root container; код приложения их не выставляет.
Сборка без native-client проверена отдельно и не требует GUI библиотек.
Native Windows/macOS/ARM64 и реальный IdP внутри webview не проверены.

Context7 review: CONTEXT7_REVIEW.md. Добавлены regression tests для IPC/navigation,
фильтра окружения и восстановления сохранения settings после stale tmp.
