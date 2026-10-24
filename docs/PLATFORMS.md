# Платформы ForgePlan 0.11.1 Alpha

Windows, macOS и Linux включены в целевую матрицу приложения. Версия 0.11
добавляет native-client feature: отдельное окно, локальную/корпоративную сессии и
встроенный webview. Документация: NATIVE_CLIENT.md. Старый desktop/browser launcher
сохраняется для сборки без GUI feature. Native client локально проверен на Linux x64.

| Платформа | Архитектура | Native target | Кандидат поставки | Проверка в этой среде |
|---|---|---|---|---|
| Linux GNU | x86_64 | x86_64-unknown-linux-gnu | tar.gz + shell launcher | Сборка, запуск, данные, архив, browser E2E |
| Linux GNU | ARM64 | aarch64-unknown-linux-gnu | tar.gz + shell launcher | CI подготовлен, запуск не выполнен |
| Windows | x64 | x86_64-pc-windows-msvc | ZIP + GUI executable/launcher | CI подготовлен, запуск не выполнен |
| macOS Intel | x86_64 | x86_64-apple-darwin | tar.gz + .app bundle/launcher | CI подготовлен, запуск не выполнен |
| macOS Apple Silicon | ARM64 | aarch64-apple-darwin | tar.gz + .app bundle/launcher | CI подготовлен, запуск не выполнен |

Матрица-источник: packaging/platforms.json. CI использует нативные runners, запускает
Rust/HTTP/workspace/platform тесты и только затем упаковывает кандидат. Наличие
workflow не означает успешного удалённого прогона. Репозиторий/Actions здесь не
публиковались. Отдельные ARM64 Windows и 32-bit targets пока вне матрицы.

## Запуск

Установленная Rust toolchain с C compiler нужна для сборки из исходников:

```sh
cargo build --release --locked
```

Windows: Rust MSVC toolchain и Visual Studio Build Tools с C++/Windows SDK.
macOS: Xcode Command Line Tools. Linux: C compiler и libc development headers.
SQLite встроен, системная SQLite DLL и внешний сервер БД не требуются.
Готовому кандидату Rust/Cargo/Python не нужны. Native Linux требует GTK3/WebKitGTK
4.1 runtime, Windows — WebView2, macOS использует системный WKWebView. Сборка без
native-client требует браузер для команды desktop.
MSVC CRT линкуется статически через .cargo/config.toml; чистая Windows-машина
ещё не проверена. Для macOS CI выставляет MACOSX_DEPLOYMENT_TARGET=13.0, но проверка
минимальной версии ОС остаётся release gate. GNU Linux ABI зависит от машины сборки.
Приложенный Linux x64 binary собран на glibc 2.39: он не объявляется совместимым с
Ubuntu 22.04/glibc 2.35. CI использует Ubuntu 22.04 для будущих Linux кандидатов.

Windows PowerShell:

```powershell
.\target\release\forgeplan.exe desktop
.\target\release\forgeplan.exe paths
```

macOS/Linux:

```sh
./target/release/forgeplan desktop
./target/release/forgeplan paths
```

`desktop [PORT] [DB] [--no-browser]` открывает браузер только после успешного bind.
Порт по умолчанию 8787; занятый порт возвращает ошибку. Можно выбрать, например,
`desktop 8788`; `desktop 0 --no-browser` выбирает свободный порт и печатает URL.
На Linux требуется xdg-open для автоматического запуска браузера. Если handler
недоступен, сервер продолжает работать, URL можно открыть вручную.
Desktop принимает только loopback; для сервера с proxy используйте `workspace`.

## Данные и обновление

| ОС | Каталог по умолчанию для desktop |
|---|---|
| Windows | %LOCALAPPDATA%\ForgePlan |
| macOS | ~/Library/Application Support/ForgePlan |
| Linux | $XDG_DATA_HOME/forgeplan, иначе ~/.local/share/forgeplan |

Файл — forgeplan.sqlite. FORGEPLAN_DATA_DIR переопределяет каталог и должен быть
абсолютным. Пустой override игнорируется; относительный XDG_DATA_HOME игнорируется.
Новый Unix-каталог создаётся с правами 0700 (существующие права не меняются).
`paths` печатает пути JSON и не создаёт каталог. Произвольный DB path из CLI
имеет приоритет; его родительский каталог должен существовать. `workspace` сохраняет
прежнюю семантику: DB относительно текущего каталога, а не OS data directory.

Данные 0.9 из forgeplan.sqlite в рабочем каталоге автоматически не переносятся:
передайте прежний путь явно, например `desktop 8787 /absolute/path/forgeplan.sqlite`.
Схема SQLite остаётся version 1; перестройки данных для этого обновления нет.
Перед обновлением остановите приложение и сделайте backup. Распаковывайте новую
сборку в отдельный каталог; данные пользователя остаются отдельно. Удаление пакета
не удаляет проекты. WAL-файлы нельзя копировать отдельно от работающей БД;
используйте `forgeplan backup DB DEST`.

## Корпоративное подключение

```sh
forgeplan connect https://projects.example.org
```

Команда открывает web-клиент корпоративного сервера в системном браузере и не
создаёт локальную БД. Разрешён HTTPS; HTTP — только для localhost/127.0.0.1/::1.
Не передавайте секреты в URL: credentials, query и fragment отклоняются.
Пока поддержан только root URL; сервер за subpath не поддержан. Токены через CLI
не передаются и не сохраняются. Пользователь проходит OIDC-вход в web-клиенте.
Это браузерный онлайн-клиент CLI; новый app имеет корпоративный режим внутри окна.
Offline pull/push остаётся отдельным CLI-механизмом.

## Упаковка и проверка

```sh
python3 scripts/package-release.py --platform linux-x86_64 --binary target/release/forgeplan
```

На Windows используйте `python` и `target/release/forgeplan.exe`; platform IDs —
в таблице packaging/platforms.json. Скрипт исполняет build-info бинарника, проверяет его версию/
точный Rust target/OS/architecture, включает notices/licenses/examples/docs/schemas, manifest.json
с SHA-256 файлов и checksum архива. Существующий output не перезаписывается.
Это portable package (для native macOS — .app внутри tar.gz), не MSI/DMG/AppImage. Подпись, notarization, auto-update,
keychain, installer integration и чистые VM проверки ещё не реализованы.
Checksum подтверждает целостность, не подлинность издателя. При отказе Gatekeeper/
SmartScreen использовать непроверенный кандидат для production нельзя; настройка
подписанного выпуска — отдельная задача. Проверка архива и извлечённого приложения:
`python3 tests/platform_smoke.py` после сборки; браузер при тесте не открывается.

Источники платформенной матрицы:
- https://doc.rust-lang.org/rustc/platform-support.html
- https://docs.github.com/en/actions/reference/runners/github-hosted-runners
