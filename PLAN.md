# План реализации

Актуальное состояние — ROADMAP_STATUS.md. Целевой объём — SPEC.md и COMPATIBILITY.csv.

Версии 0.1…0.4 создали календарное ядро, WBS и анализ ресурсов. Версия 0.9 Alpha
объединила проход по следующим этапам: native JSON, work modes, календари мощности,
стоимость/факт/baselines/EVM, leveling, persistence, multi-project workspace,
org roles, OIDC, publish/audit/timesheets, offline CLI, interchange и build recipes.

Работа продолжается по зависимостям, без согласования каждого перехода. Не включать
в release claims непроверенные платформы и неподдерживаемые форматы.

Версия 0.10 добавляет platform data directories, native CI/portable packages для
Windows x64, macOS Intel/ARM64 и Linux x64/ARM64, corporate connect и platform tests.
Локально проверен Linux x64; остальные платформы ждут выполнения native runners.

Версия 0.11 добавляет native окно с автономным и корпоративным режимами.
Следующий клиентский пакет: серверные offline-копии, outbox, конфликты/merge,
keychain и нативные Windows/macOS/Wayland проверки.

Ближайшие acceptance gates:

- PostgreSQL: две независимые сессии, stale revision, duplicate command_id, restart,
  транзакционный audit/receipt, backup/restore и tenant isolation.
- Assignment engine: resource calendar intersection, DST transitions, work/duration/
  units matrix и effort-driven scenarios с эталоном Project.
- UI: виртуализация 10000 задач, клавиатура, Gantt edit/link/zoom, WCAG audit.
- Desktop: Windows/macOS/Linux packages, local keychain, offline outbox/merge,
  signed update and platform E2E.
- Interchange: собственный corpus и MS Project differential tests; loss report
  для каждого неподдержанного поля; MPP не объявлять поддержанным до проверки.
- Production: workload profile, cancellation/quotas, fuzzing, dependency/security
  audit, operational restore exercise и утверждённая release checklist.
