# Native document schema 1

.fplan — UTF-8 JSON:

```json
{"format":"forgeplan","schema_version":1,"project":{}}
```

project должен соответствовать schemas/forgeplan.schema.json; пустой объект
выше — только иллюстрация envelope, не валидный проект. Unknown fields и будущая
schema_version отклоняются. Struct/map/newtype deserialization дополняется проверкой
графа, календарей, ссылок, денежных переполнений и доменных инвариантов Rust.
Canonical encode сортирует WBS, links, resources, periods и assignments.
ID values сериализуются числами, map keys — строками; числовые string IDs также
принимаются для корректной десериализации tagged command buffers, но проверяются.

Старые TSV forgeplan<TAB>1…4 принимаются автоматически. V1/V2 длительности/lag
умножаются на 480; V3/V4 уже минутные. JSON сохраняет calendars, resources,
management policies, baselines, costs и actuals. Файл до 8 MB, без Editor history,
revision и used IDs — эти данные сохраняет workspace DB. Полный JSON не заменяется
ограниченным CSV/XML.

XML: hierarchy, durations PTnHnMn0S, links (tenths-of-minute lag), work resources и
assignments. Default calendar Mon–Fri 09:00–17:00 / UTC+00; unsupported XML fields
и календарные/управленческие потери диагностируются. DTD запрещён. Native roundtrip
сохраняет всю модель; XML acceptance в Microsoft Project не проверен.
CSV: строго id,parent,order,kind,title,duration_minutes,start,finish. Импортирует
только структуру/duration, даты пересчитывает от явно объявленного default calendar;
не импортирует links/resources/costs. Формулы в export именах защищаются апострофом.
Подтверждение импорта после предупреждений обязательно в web workflow.
