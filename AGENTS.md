# Работа над ForgePlan

## Документация библиотек через Context7

При изменении библиотек, framework/API usage или платформенной интеграции:

1. Определить фактическую версию в Cargo.lock и соответствующий feature set.
2. Использовать плагин Context7: сначала resolve-library-id, затем query-docs.
   Запрашивать конкретный API/поведение, не отправлять код проекта, токены,
   пользовательские данные и настройки корпоративных систем.
3. Сопоставить результат с документацией/исходниками закреплённой версии.
   Context7 может индексировать dev/master: это не доказательство совместимости
   с нашей версией и не повод автоматически обновлять зависимости.
4. Исправления проверять компиляцией, clippy и тестами соответствующего сценария.
   Для native-client отдельно проверять GUI feature и headless server build.
5. В существенных изменениях фиксировать library IDs, версии, источники, выводы
   и фактически выполненные проверки в docs/. Не выдавать CI recipe за native run.

Если Context7 недоступен/не покрывает API, использовать официальную документацию
или Cargo registry source закреплённой версии и отметить этот fallback в отчёте.
Не блокировать разрешённую работу из-за недоступности плагина.

Context7 используется в разработке/ревью. Приложение и сервер не требуют
подключения к плагину, внешних documentation API или его ключей для запуска.

## Проверки

- cargo fmt --all -- --check
- cargo clippy --workspace --all-targets --locked -- -D warnings
- cargo test --workspace --locked
- Повторить clippy/test с --features native-client для изменений клиента.
- Build release для соответствующего feature set перед integration/E2E.
- tests/http_smoke.py, workspace_smoke.py, platform_smoke.py; OIDC при изменении auth.
- Для native lifecycle/navigation/UI: tests/native_e2e.py под Linux/Xvfb.

Не объявлять Windows/macOS/ARM64 проверенными по результатам Linux или наличию
workflow. Не включать QA screenshots, токены и рабочие SQLite в поставку.
