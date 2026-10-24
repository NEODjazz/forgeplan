# Команды и история 0.9

Editor применяет группу к копии, проверяет модель/расписание, затем увеличивает
revision. Ошибка сохраняет проект, ID allocators и историю. Undo/Redo увеличивают
revision монотонно. Удалённые IDs не переиспользуются в сессии; workspace snapshot
сохраняет allocators и историю после перезапуска.

Нативный JSON command envelope: docs/API.md. Сохраняется старый текстовый wire с
первой строкой revision<TAB>N; отдельная строка `json<TAB>{tagged Command}` добавляет
типизированную команду в ту же atomic group. Не смешивать разные единицы старых
дневных версий: wire values сейчас минуты и basis points, не UI проценты.

Новые команды: Policy / Work / PersonalCalendar / Rate / FixedCost / Actual /
Baseline / StatusDate. Их изменения поддерживают Undo и durable CAS/idempotency.
Replace — полный Project; native file envelope предварительно декодируется parse.
Baseline одинакового существующего имени нельзя менять в месте; новый import может
удалить старую baseline, явно заменяя документ. Import доступен Undo.

Leveling preview не мутирует Project; workspace apply пересчитывает preview на
base revision и отклоняет stale/unresolved. Durable receipts возвращают исходный
response на повтор command_id/actor/payload, даже после следующих команд.
История ограничена 50 снимками и persistence byte budget; старые снимки сокращаются.
