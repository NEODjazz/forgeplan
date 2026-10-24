# Изменения

## 0.11.1 — 24.10.2026

Context7-assisted API review (Wry/Tao/rusqlite), documented workflow in AGENTS.md
and evidence in docs/CONTEXT7_REVIEW.md. Unique settings temp files recover from
stale previous writes; inherited app settings are filtered case-insensitively.
Home/local editor use incognito instead of relying on cross-platform data-dir
semantics. Added IPC/navigation and settings/environment regression coverage.
SQLite format and scheduling semantics unchanged. Linux verified; other OS runs pending.


## 0.11.0 — 24.10.2026

Optional Rust native-client (Tao/Wry) with system window, local/corporate chooser,
owned loopback process and ephemeral 256-bit API token, profile directories and
last-server preferences. Native editor shares the existing UI/core/backend;
Home/Quit stops the owned process, remote workspaces remain independent.
F6 Home, Ctrl/Cmd+S apply, Ctrl/Cmd+Q quit. Server builds retain no GUI dependency.
Native Linux X11 window E2E covers offline edit/restart, private API, lifecycle,
corporate connection and URL validation. Portable packages select native launch,
macOS recipe produces unsigned .app. Native Windows/macOS CI not run here.
No automatic offline cache/outbox/merge/sync, signed installer or real-tenant SSO claim.


## 0.10.0 — 24.10.2026

Native platform matrix (Windows x64, macOS Intel/Apple Silicon, Linux x64/ARM64),
OS user data paths, private Unix directory creation, desktop readiness-based
browser launch and headless mode. Corporate connect validates HTTPS URLs and
opens the system browser without creating a DB. Platform/version diagnostics.
Portable archive builder validates the candidate platform/version and includes
license notices, manifest/checksums, launchers. Native CI packages all matrix
entries after tests. Linux x64 verified locally; other runners not executed.
SQLite schema remains version 1; existing CWD databases are not moved automatically.
No native webview, signed installer, updater or claim of full MS Project parity.


## 0.9.0 Alpha — 24.10.2026

Consolidated roadmap pass: native JSON/schema; work modes and capacity calendars;
costs/baselines/actual/EVM; bounded leveling; SQLite persistence/audit/receipts/
publish/timesheets; organization roles and OIDC/JWKS/PKCE; multi-project web;
offline pull/push; XML/CSV loss diagnostics; deployment/build recipes.
Not production parity; PostgreSQL/native signed clients/MPP and other items remain.


## 0.4.0 — 24.10.2026

Person/generic ресурсы, Units/ResourceId, availability periods, assignments,
exact planned work и summary rollup, sweep нагрузки/перегрузок, UI/resources,
удаление с восстановлением назначений, .fplan v4 и чтение v1/v2/v3.
Назначения не изменяют расписание; leveling и resource calendars не реализованы.


## 0.3.0 — 24.10.2026

Working-minute Timeline, week intervals, breaks/exceptions, split overnight,
fixed UTC offset, six date constraints, soft deadlines, driving reasons,
UI calendar/date fields, .fplan v3 and v1/v2 migration (1 day = 480 minutes).
Start/finish JSON теперь timestamps; duration/lag/float и command values — минуты.
Deadlines не изменяют float. IANA/DST, ресурсы и JSON-контейнер не реализованы.


## 0.2.0 — 24.10.2026

Добавлены типизированные ID/длительности/лаги/ревизии; WBS и итоговые этапы;
порядок соседей; перемещение поддерева; атомарные команды; Undo/Redo;
защита от устаревших вкладок; редактор структуры; file v2 и миграция v1.

Поведение изменилось: GET /api/project возвращает текущий применённый проект,
а не исходный файл. POST /api/schedule остался чистой проверкой без записи.
История/ревизии теперь находятся в памяти Rust-сервера. Файлы по умолчанию
сохраняются в формате v2, который ForgePlan 0.1 не читает.

## 0.1.0

Первое whole-day CPM ядро, плоские задачи, CLI, локальная веб-форма, файл v1,
исходная спецификация и матрица функций.
