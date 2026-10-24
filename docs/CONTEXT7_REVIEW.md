# Проверка ForgePlan 0.11.1 с Context7

Дата: 24.10.2026. Проверен native-клиент 0.11, локальная сессия, настройки,
границы IPC/навигации, транзакционный слой SQLite и сборка двух feature sets.
Это ограниченное кодовое ревью с регрессионными тестами, не production pentest
и не подтверждение полного паритета с Microsoft Project.

## Использование плагина

В AGENTS.md закреплён процесс resolve-library-id → query-docs → сверка версии →
проверка кода. В этой проверке выполнены 3 resolve и 3 query запроса Context7.

| Библиотека | Версия Cargo.lock | Context7 library ID | Что проверялось |
|---|---|---|---|
| Wry | 0.57.1 | /tauri-apps/wry | Navigation/IPC при переходах local/remote |
| Tao | 0.37.2 | /tauri-apps/tao | Завершение event loop и GTK container |
| rusqlite | 0.37.0 | /rusqlite/rusqlite | IMMEDIATE transaction, commit и rollback |

Context7 вернул материалы dev/master, а не snapshots наших версий. Сигнатуры и
платформенное поведение дополнительно сопоставлены с исходниками этих версий из
Cargo registry. Результат документационного поиска сам по себе не считается
доказательством корректности. Документация получала только общие вопросы об API,
без кода проекта, персональных данных и секретов.

## Найденное и исправленное

| Находка | Влияние | Исправление и доказательство |
|---|---|---|
| Постоянное имя client-settings.json.tmp | Оставшийся после сбоя файл блокировал следующее сохранение адреса сервера | Уникальный временный файл, create_new, sync и rename; тест оставляет старый tmp и проверяет два успешных сохранения/замены |
| Регистрозависимый фильтр FORGEPLAN_* | В Windows унаследованные настройки с другим регистром могли влиять на локальный процесс, включая bind/OIDC | ASCII case-insensitive фильтр; тесты трёх вариантов регистра и переменных, которые требуется сохранить; Windows runtime не проверен |
| Изоляция локального webview опиралась на data directory | Контракт разных каталогов нельзя переносить на все OS engines: WKWebView может использовать общий default data store | Локальный editor и стартовый экран явно используют incognito; сверены реализация Wry для GTK/WKWebView и Linux GUI regression |

Данные SQLite не мигрировались. Настройки по-прежнему содержат только адрес
сервера; прежний фиксированный tmp не удаляется автоматически и не блокирует
дальнейшее сохранение. Корпоративная webview сохраняет штатное поведение профиля.

## Проверенные границы

- В editor IPC доступен только Home: local/remote/file-like дополнительные поля,
  неизвестные команды, не-JSON и слишком длинные сообщения отклоняются. Выделен
  проверяемый обработчик; добавлен regression test.
- Локальная навигация остаётся на exact origin, включая порт. Внешние страницы,
  другой loopback port, file/javascript и URL с credentials блокируются. В remote
  разрешены HTTPS-переходы (включая IdP), HTTP только к исходному loopback origin.
- По документации Wry на Linux URL IPC из iframe может соответствовать main frame.
  Защита editor не строится на доверии к request URL: список действий ограничен Home.
- GTK использует default_vbox контейнер Tao. Перед Exit явно освобождаются webview
  и LocalSession, а owned process завершается; тесты Home/Quit подтверждают это.
- rusqlite Transaction по умолчанию откатывает незакоммиченные изменения. Код
  применяет IMMEDIATE, CAS, audit/receipt и commit в одной транзакции; проверки
  restart/conflict/idempotency/backup сохранены. Дефект в этой семантике не найден.

## Проверки

Фактические результаты текущей сборки перечислены в VALIDATION.md. Windows,
macOS, ARM64, Wayland и реальные IdP tenants этой сессией не проверены. Наличие
Context7 и native CI не заменяет эти прогоны. Полный advisory/dependency scan,
нагрузочный аудит и тесты аварийного завершения клиентского процесса не выполнены.
Known limits: нет автоматических offline-копий серверных проектов/outbox/merge;
нет single-instance guard и crash orphan recovery; нет подписанных установщиков.

## Источники

Материалы, возвращённые Context7:
- https://github.com/tauri-apps/wry/blob/dev/_autodocs/api-reference/WebViewBuilder.md
- https://github.com/tauri-apps/wry/blob/dev/_autodocs/endpoints.md
- https://github.com/tauri-apps/tao/blob/dev/_autodocs/api-reference/event-loop.md
- https://github.com/tauri-apps/tao/blob/dev/_autodocs/api-reference/platform-unix.md
- https://github.com/rusqlite/rusqlite/blob/master/_autodocs/api-reference/transaction.md

Закреплённые версии для проверки API:
- https://docs.rs/wry/0.57.1/wry/
- https://docs.rs/tao/0.37.2/tao/
- https://docs.rs/rusqlite/0.37.0/rusqlite/
