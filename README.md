# ForgePlan 0.11.1 Alpha

Open-source система управления проектами на Rust. Этот выпуск объединяет несколько
этапов дорожной карты. Это **alpha**, а не полный проверенный аналог Microsoft Project.
Точные границы: [ROADMAP_STATUS.md](ROADMAP_STATUS.md), [COMPATIBILITY.csv](COMPATIBILITY.csv).

## Клиентское приложение

Теперь есть отдельное native окно: **Локальные проекты / Корпоративный сервер**.
Локальный режим работает без интернета и корпоративного сервера, хранит проекты
на компьютере. Корпоративный режим открывает сервер внутри окна клиента.
Подробности и ограничения: [docs/NATIVE_CLIENT.md](docs/NATIVE_CLIENT.md).

```sh
cargo run -p forgeplan --features native-client -- app
```

Для Linux GUI требуются GTK3/WebKitGTK 4.1 development packages при сборке и runtime
libraries при запуске. В архиве готовый Linux x64 native binary (glibc 2.39).
Windows/macOS native CI и packaging подготовлены; здесь эти ОС не проверены.
`dist/linux-x86_64/forgeplan` — клиент, `forgeplan-server` — сервер без GUI-зависимостей.
Локальные и серверные проекты независимы: автоматической синхронизации пока нет.

## Запуск с постоянным хранением

Из папки проекта:

```sh
cargo run -p forgeplan -- workspace 8787 forgeplan.sqlite
```

Открыть **http://127.0.0.1:8787**. Проекты, история Undo/Redo, публикации, аудит и
табели сохраняются в SQLite. После перезапуска данные остаются. Отдельная внешняя
БД для локального режима не нужна. Зависимости зафиксированы Cargo.lock.

Поддерживаемая целевая матрица: **Windows x64, macOS Intel/Apple Silicon,
Linux x64/ARM64**. В этой среде выполнен только Linux x64 прогон; для остальных
подготовлены native CI и portable packaging. Готовый Linux x64 пакет находится
в `dist/`; текущий binary требует glibc 2.39. Подробности: [docs/PLATFORMS.md](docs/PLATFORMS.md).

```sh
./dist/linux-x86_64/forgeplan-server workspace 8787 forgeplan.sqlite
# Либо launcher с открытием системного браузера:
cargo run -p forgeplan -- desktop 8787 forgeplan.sqlite
```

Команда `desktop` сохраняет прежний браузерный launcher. Новый `app` открывает
системное окно со встроенным webview; native feature включается отдельно.
Старый `serve [PORT] [FILE]` остаётся временным однопроектным режимом без дисковой записи.

## Что работает

- WBS, итоговые задачи, рабочие минуты/смены/исключения, FS/SS/FF/SF, лаги,
  ограничения дат, критические задачи, резервы, пояснения и дедлайны.
- Люди/роли, доступность, назначения, точная работа, дневная нагрузка и перегрузки.
- Режимы fixed-duration/fixed-work/fixed-units и effort-driven при изменении
  работы/назначений. Правила и округления alpha описаны в docs/MANAGEMENT.md.
- Персональные недельные календари мощности; они не распределяют работу по
  отдельным ресурсам автоматически и не заменяют полноценный assignment scheduler.
- Ставки, фиксированные затраты, факт/процент выполнения, базовые планы и EVM.
- Явное выравнивание: предпросмотр → применение только решённого плана → Undo.
- Несколько проектов, обзор портфеля, публикации ревизий, аудит, роли и табели.
- OIDC discovery/JWKS/RS256, issuer/audience/exp, rotation; web Authorization Code+PKCE.
- JSON со схемой и миграциями TSV v1…v4; ограниченный XML/CSV с диагностикой потерь.
- Offline JSON pull/push с optimistic concurrency и резервное копирование SQLite.

## Данные и единицы

Длительности/лаги — целые рабочие минуты. В UI назначения — проценты; в API — basis
points (10000=100%). Стоимость — целые сотые одной условной валюты проекта; разные
валюты и FX не поддержаны. JSON schema: schemas/forgeplan.schema.json.
Нативный .fplan теперь JSON: format=forgeplan, schema_version=1.

Не редактируйте XML/CSV как замену полной модели: поддержан только поднабор, потери
показываются до применения. Модели всех предыдущих версий читаются; сохранение — JSON.
Для примера с базовым планом, ставками и фактом откройте `examples/managed-project.fplan`.
Все имена и суммы в примерах синтетические.

## Команды

```sh
cargo test --workspace --locked
cargo build --release --locked
cargo run -p forgeplan -- desktop
cargo run -p forgeplan -- paths
cargo run -p forgeplan -- connect https://projects.example.org
cargo run -p forgeplan -- demo my-project.fplan
cargo run -p forgeplan -- check my-project.fplan
cargo run -p forgeplan -- convert old.fplan new.fplan
cargo run -p forgeplan -- backup forgeplan.sqlite backup.sqlite
cargo run -p forgeplan -- pull http://127.0.0.1:8787 local demo offline.fplan
cargo run -p forgeplan -- push offline.fplan
```

pull создаёт также .sync.json с базовой ревизией, но без токена. Для авторизованного
сервера задайте FORGEPLAN_TOKEN. При конфликте push возвращает ошибку и сохраняет
локальный файл. Это CLI-обмен целой моделью; автоматического merge/outbox пока нет.

## Доступ

Без настройки авторизации сервер работает как локальная dev-сессия администратора.
Для команды используйте OIDC или FORGEPLAN_IDENTITIES (JSON token→identity).
Роли viewer/editor/manager/admin изолированы по организации. Project-level ACL и
сопоставление пользователей сотрудникам/ресурсам ещё не реализованы.
Настройка Keycloak/Entra-compatible IdP и ограничения: [docs/AUTH.md](docs/AUTH.md).

По умолчанию сервер слушает только loopback. Внешняя привязка требует авторизацию
и явно настроенный public origin; TLS завершает reverse proxy. Docker/Compose
подготовлены, но в этой среде не собирались. Детали: docs/WORKSPACE.md.

## Границы alpha

Нет PostgreSQL backend, IANA/DST, полного MS Project расчёта, межпроектного пула
ресурсов/зависимостей, материальных/стоимостных ресурсов, полноценного workflow
портфелей, MPP/VBA, полной виртуализации UI и подписанных установщиков.
SSO проверен локальным mock IdP, не реальным tenant. Security/performance/release
аудит и differential tests с Microsoft Project не выполнены.

Desktop хранит БД в системном каталоге пользователя, а `workspace` — по явно
переданному пути. Для прежней БД укажите её путь: автоматического переноса нет.

Лицензия Apache-2.0. docs/DEPENDENCIES.csv и third_party_licenses/ фиксируют зависимости
и notices. Источники и бинарник не опубликованы в GitHub/внешний хостинг.

Проверка с Context7 и исправления 0.11.1: [docs/CONTEXT7_REVIEW.md](docs/CONTEXT7_REVIEW.md).
Правила работы с документацией библиотек: [AGENTS.md](AGENTS.md).
