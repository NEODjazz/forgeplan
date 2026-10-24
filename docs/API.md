# Workspace API 0.9 Alpha

OpenAPI: schemas/openapi.json или GET /api/openapi.json. Native JSON schema:
GET /api/schema.json. Model boundary semantics: docs/SCHEDULING.md, MANAGEMENT.md.
Default host 127.0.0.1:8787. Headers X-Organization (local) / X-Project-ID (demo).
Bearer authentication optional только для loopback development; static/OIDC — AUTH.md.

| Route | Поведение |
|---|---|
| GET /api/auth/config | Public OIDC browser metadata или null |
| GET /api/capabilities | Подтверждённые role/subject/org и persistent mode |
| GET/POST /api/projects | Список / создание (admin) |
| GET /api/portfolio | Обзор проектов и planned costs, не portfolio optimizer |
| GET /api/state, /api/project | Editor response / native document |
| POST /api/commands | Atomic JSON command envelope или legacy text commands |
| POST /api/schedule | Pure calculation, без записи |
| GET /api/audit | Последние 100 событий проекта |
| POST /api/publish, GET /api/published | Publish current revision / latest published snapshot |
| POST /api/leveling/preview, /apply | Preview; apply только решённой актуальной ревизии |
| GET /api/export.csv, /api/export.xml | Task CSV / XML subset + warnings |
| POST /api/import.csv, /api/import.xml | Preview document + loss warnings, без записи |
| GET/POST /api/timesheets | Последние 200 / submitted time |
| POST /api/timesheets/approve | Independent manager approval + actual update |

POST проверяет Origin; чтение схем/auth config публичное, данные проектов защищены.
X-Command-ID обязателен для durable commands/level apply. JSON commands:

```json
{"base_revision":0,"commands":[{"type":"rate","data":{"id":1,"cents_per_hour":6000}}]}
```

Command enum: add/update/move/delete_subtree/set_predecessors/project_settings/
calendar_settings/dates/add_resource/update_resource/delete_resource/set_assignments/
policy/work/personal_calendar/rate/fixed_cost/actual/baseline/status_date/replace.
Data shape соответствует Rust Command; native Project в replace — без document envelope.
Undo/Redo остаются legacy envelope `revision<TAB>N` + `undo`/`redo`.
Publish body {base_revision}; level body {base_revision,within_float}; time body
{task,resource,date,minutes}; approve body {id}. Amounts — cents, target work — basis.

Коды: 401 auth, 403 role/org/self-approval, 404 missing, 409 stale revision/idempotency,
422 domain/parsing/operational failure. Транспорт HTTP/1.1, close after response,
body cap 8 MB; no chunked. Нет WebSocket push, project-level ACL или PostgreSQL adapter.
