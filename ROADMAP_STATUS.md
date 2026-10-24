# Фактическое состояние дорожной карты — 24.10.2026

Пользователь разрешил продолжать все этапы без промежуточного согласования.
**Все этапы не завершены.** Таблица отражает код и проверки, а не обещанный паритет.
Не отмечать этап завершённым только по наличию конфигурации/интерфейса.

| Этап | Реализовано и проверено | Осталось |
|---|---|---|
| P0 | Спецификация, матрица, работающий расчёт/CLI/web | Исчерпывающий эталон MS Project |
| P1 | Типы, WBS, атомарные команды/история, JSON schema, миграция TSV | Заморозка стабильного публичного API после пилота |
| P2 | Рабочие минуты, ограничения, объяснения, split shifts | IANA/DST, ALAP, ручные/inactive/split tasks, elapsed lag, free float |
| P3 | Общий web editor, ресурсы, план/факт, workspace и SSO | Виртуализация, UX/accessibility audit, полноценные Gantt interaction/виды |
| P4 | Базовые режимы work/units/duration, календари мощности, ставки, baseline/actual/EVM | Полный assignment scheduler, профили/стоимость по периодам, currencies/материалы |
| P5 | SQLite durable transactions/history/receipts, org RBAC, OIDC/PKCE, publish/audit/backup | PostgreSQL, project ACL, BFF/session/refresh lifecycle, production load/security |
| P6 | Native window/local-corporate chooser, protected owned process, OS paths, offline CLI, packaging и CI 5 targets; Linux native E2E | Keychain, offline server cache/outbox/merge, Windows/macOS/ARM64 прогоны и подписи |
| P7 | Heuristic leveling preview/apply/undo и табели с независимым approval | Общий корпоративный пул, межпроектные связи, приоритеты/оптимизация, полный approval workflow |
| P8 | Обзор проектов/портфеля, EVM, CSV/XML subset с loss diagnostics | Программы/ворота/сценарии, BI, webhooks, Jira/GitLab/HR/ERP и native XLSX |
| P9 | Лимиты/валидация, dependency inventory, auth tests, build/deployment recipes | MPP adapter, differential corpus, fuzz/performance/pentest/SBOM audit и release acceptance |

## Следующий незавершённый пакет

1. PostgreSQL repository с той же CAS/idempotency семантикой и integration tests.
2. Project-level ACL + staff/resource mapping для табелей, BFF/session refresh.
3. IANA/DST и реальное календарное распределение назначений; расширение work modes.
4. Production UI, cross-project scheduling/pool и workflow программ/портфелей.
5. Настоящая desktop packaging и подписи; проверка XML на Microsoft Project, затем MPP.

## Что требует внешнего подтверждения

Реальный IdP tenant/client/roles/redirect URI; исходный Git-репозиторий/CI runner;
Windows/macOS build/signing environment; эталонные XML/MPP и доступ к MS Project.
Отсутствие этих данных не превращает неготовую функцию в выполненную. Генерируемые
Docker/CI файлы также не являются доказательством успешного deployment/build.
