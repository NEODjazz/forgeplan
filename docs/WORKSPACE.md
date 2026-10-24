# Persistent workspace и поставка

`forgeplan workspace PORT DB` использует SQLite WAL, schema version 1 из
migrations/001_sqlite.sql. PostgreSQL schema в корне — старый target sketch,
не runtime backend. Неподдерживаемая версия SQLite schema блокирует запуск.

Store хранит Editor snapshot (включая Undo/Redo и used IDs), audit, receipts,
publications и timesheets. Изменение = BEGIN IMMEDIATE → expected revision →
validated commands → snapshot/revision/audit/receipt → COMMIT. Команда с тем же
X-Command-ID/actor и payload возвращает исходный результат; другой payload — 409.
История до 50 снимков, при большом объёме старые снимки автоматически сокращаются
под persistence budget. Максимум snapshot 64 MB; новый Project JSON — до 8 MB.

Organization определяется проверенной identity и сравнивается с X-Organization.
Project scope — X-Project-ID. Viewer читает; editor редактирует/подаёт табели;
manager публикует/согласует; admin создаёт проекты. Это org-scoped RBAC,
не project-level ACL. Tokens не выводятся в логи и не хранятся в .sync.json.

Табель требует существующее назначение, 1…1440 минут и дату. Утверждать может
manager/admin, отличный от автора. Повторное approval отклоняется. Work и cost
добавляются в actual в транзакции; стоимость считается по текущей ставке с
округлением до сотых. Нет staff-to-resource identity mapping и rejection workflow.

`forgeplan backup DB DEST` использует SQLite online backup API и не перезаписывает
DEST. Для восстановления остановите сервер и запустите workspace с файлом backup
как DB. API keys/IdP configuration в БД не хранятся, их восстанавливает оператор.
CLI pull/push работает только с loopback HTTP: base revision защищает от overwrite,
токен берётся из FORGEPLAN_TOKEN, automatic merge отсутствует. Push использует
идемпотентный digest команды; конфликт оставляет файл нетронутым.

## Docker и reverse proxy

Dockerfile/compose.yaml подготовлены, но здесь Docker отсутствует — сборка не проверена.
Compose публикует порт на 127.0.0.1 и требует FORGEPLAN_IDENTITIES JSON. Например
структура (замените TOKEN на длинный случайный секрет, не используйте буквально):

```json
{"TOKEN":{"subject":"admin","organization":"local","role":"admin"}}
```

Для прямой настройки: FORGEPLAN_BIND=0.0.0.0 требует авторизацию (static/OIDC).
FORGEPLAN_PUBLIC_ORIGIN — точный origin браузера. Для remote допускается HTTPS;
TLS завершает proxy, Host/Origin должны совпадать. Для loopback Docker допускается
http://127.0.0.1:PORT или http://localhost:PORT. Без env bind только 127.0.0.1.

Сервер пока синхронный; нет production load/cancellation/metrics audit.
Desktop launcher открывает браузер после bind и запускает workspace с OS data path.
Corporate connect открывает URL сервера без локальной БД. Матрица и native packaging
описаны в PLATFORMS.md; локально проверен Linux x64, остальные native runners и
signatures ещё не проверены.
