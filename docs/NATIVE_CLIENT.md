# ForgePlan Native Client 0.11.1 Alpha

Отдельное приложение с системным окном на Rust (Tao/Wry) и встроенным webview.
Интерфейс редактора общий с web-клиентом. Системный браузер для основного окна
не запускается. Стартовый экран предлагает «Локальные проекты» и «Корпоративный сервер».

## Локально, без корпоративного сервера и интернета

При выборе локального режима приложение запускает собственный вычислительный процесс
из того же executable. Он слушает только 127.0.0.1 на свободном порту, использует
SQLite и требует случайный 256-bit токен текущей сессии. Клиент передаёт токен
локальному webview; вводить его пользователю не нужно. Токен не записывается в
client-settings.json и не выводится в диагностике. Унаследованные FORGEPLAN_*
настройки сервера удаляются из окружения этого дочернего процесса, чтобы внешний
bind/OIDC не меняли поведение локального режима.

Локальные проекты доступны без сети: WBS, задачи/связи/календари, расчёт, ресурсы,
базовые планы, затраты/факт/EVM, импорт/экспорт и история используют тот же backend.
Данные сохраняются в forgeplan.sqlite в OS data directory (см. PLATFORMS.md).
Путь с пробелами и кириллицей проверен. App использует эту базу; старая БД из cwd
автоматически не импортируется. Для другой базы используйте абсолютный
FORGEPLAN_DATA_DIR и остановленную резервную копию, либо .fplan import.

Нормальное закрытие окна, Ctrl/Cmd+Q и возврат на стартовый экран завершают owned
процесс. SQLite восстанавливает WAL при следующем запуске. Черновые изменения
сначала применяйте кнопкой редактора или Ctrl/Cmd+S. Аварийное убийство самого
клиента может оставить дочерний процесс; OS job/parent-death recovery и механизм
единственного экземпляра ещё не реализованы.

## Корпоративный режим

Введите корневой HTTPS URL сервера. Для локальных стендов разрешён loopback HTTP.
URL с credentials, query, fragment или subpath отклоняется. Последний корректный
адрес запоминается в client-settings.json; токены туда не записываются.
Встроенный webview открывает сервер, вход выполняется средствами web-клиента
(static token/OIDC). Серверное содержимое не получает native-команд для запуска
локального backend или доступа к файлам: единственное IPC действие — возврат домой.
Окна/попапы window.open заблокированы; текущий OIDC redirect flow использует основное
окно. Реальные tenant policies могут запрещать embedded login — такие интеграции
не проверены. Corporate cookies/profile хранятся средствами OS webview, keychain/BFF
и очистка всех IdP cookies при logout ещё не реализованы.

Локальная и серверная БД независимы. Серверные проекты автоматически не становятся
доступны без связи. Нет автоматической offline-копии, очереди изменений, merge или
синхронизации между режимами. CLI pull/push из предыдущих версий сохраняется как
отдельный механизм с прежними ограничениями; это не фоновая синхронизация клиента.
При недоступном сервере клавиша F6 возвращает к выбору режима.

## Управление

- F6 / кнопка «Режим работы» — стартовый экран; локальная сессия останавливается.
- Ctrl+S (Cmd+S на macOS) — применить изменения в редакторе.
- Ctrl+Q (Cmd+Q на macOS) / закрыть окно — завершить приложение.

Windows/macOS shortcuts и platform-specific поведение требуют native acceptance;
на этой среде проверен Linux X11/WebKitGTK. Clipboard, file dialogs, downloads,
SSO с реальным IdP и Wayland остаются расширенными платформенными проверками.

## Сборка и запуск

```sh
cargo build --release --features native-client --locked
cargo run -p forgeplan --features native-client -- app
```

Готовый native executable запускается также без аргументов. Для сборки на Linux
нужны pkg-config, GTK3/WebKitGTK 4.1/DBus development packages и C compiler. На
Ubuntu: libgtk-3-dev libwebkit2gtk-4.1-dev libdbus-1-dev pkg-config. При запуске
нужны GTK3 и WebKitGTK 4.1 runtime libraries; они не входят в portable archive.
Windows использует WebView2 Runtime; macOS — системный WKWebView.

Native feature опциональна: сервер/Docker можно собирать без GUI-зависимостей
обычной `cargo build --release --locked`. Старый `desktop` открывает системный
браузер; новый `app` открывает native window. `connect` сохраняет прежнюю CLI
семантику внешнего браузера; корпоративный режим внутри app выбирается в окне.

Поставки: Linux portable tar.gz, Windows ZIP с GUI executable, macOS .app bundle
в tar.gz. Подпись, notarization, MSI/DMG installers и автообновление не выполнены.
Mac bundle содержит ATS exception только для локальной сети/127.0.0.1; внешний
сервер по-прежнему требует HTTPS. macOS/Windows native build jobs подготовлены,
в этой среде не выполнялись.

## Проверка

Rust unit suite с native-client и clippy; Linux native window E2E через Xvfb/
xdotool, без тестового native API. Проверены редактирование через окно, Ctrl+S,
restart, приватность API текущей локальной сессии, cleanup при Home/Quit,
независимый корпоративный workspace, URL validation и сохранение только адреса.

```sh
xvfb-run -a -s '-screen 0 1440x1000x24' python3 tests/native_e2e.py
```

GUI driver требует Linux/X11 и xdotool. Скриншоты — только опциональный локальный QA,
не часть исходников/поставки. Root/sandbox test environment в этой сессии использует
отдельные test-only WebKit flags; приложение не отключает sandbox в runtime-коде.

Технические источники:
- https://docs.rs/wry/0.57.1/wry/
- https://docs.rs/tao/0.37.2/tao/
- https://developer.apple.com/documentation/bundleresources/information-property-list/nsapptransportsecurity/nsallowslocalnetworking

## Изоляция webview в 0.11.1

Стартовый экран и локальный editor явно используют incognito. Поведение разных
каталогов WebContext не считается гарантией изоляции на macOS: локальная сессия
использует nonpersistent store. Корпоративная сессия сохраняет штатный профиль ОС.
Сохранение server settings использует уникальный tmp; файл от прерванного
сохранения старой версии не блокирует запись. Environment filter учитывает
регистр имён Windows. См. CONTEXT7_REVIEW.md.
