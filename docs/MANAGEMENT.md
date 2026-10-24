# Планирование работы, стоимость и контроль — alpha

TaskPolicy: fixed_duration, fixed_work, fixed_units; effort_driven; optional target
work in minute*basis_points. Work input — минуты, internal target — 1/10000 минуты.
Это определённая матрица поведения alpha, не доказанный клон Microsoft Project.

- Fixed duration: заданная длительность сохраняется; SetWork пропорционально
  масштабирует units назначений с распределением остатка.
- Fixed work: при смене назначений сохраняется target work и меняется duration;
  при явном изменении duration масштабируются units.
- Fixed units: SetWork сохраняет units и меняет duration. Обычный ввод duration
  меняет производную работу. При effort-driven смена назначений сохраняет работу.
- Duration целоминутная, units имеют точность 0,01%; округление вверх может дать
  небольшое превышение target work. Это не variable last-minute contour.

Личный календарь задаёт available capacity на пересечении с общим календарём.
Он не разносит фактические assignment segments и не продлевает работу автоматически.
Это оставшаяся часть assignment scheduler, а не скрыто реализованная оптимизация.

Ставка — cents/hour; фиксированная стоимость — cents/task. Производная стоимость
uniform work округляется вниз до целых сотых. Одна условная валюта, без FX,
rate tables/effective dates и отдельных правил начисления start/prorated/end.

Baseline сохраняет календарь, даты, work и стоимость leaf tasks. Именованный
снимок не меняется при редактировании ставок/плана; изменение существующего имени
через Replace отклоняется. Импорт нового документа может удалить старые снимки;
этому предшествует явное подтверждение и операция доступна Undo.

Actual: percent_complete, work_basis, cost_cents, optional start/finish. Факт
не перепланирует remaining work сам. Approved timesheet добавляет work/cost в actual,
но персональное сопоставление resource↔identity ещё не задано, а редакторы проекта
могут менять actual. Нельзя считать это полной защитой утверждённых фактов.

EVM: BAC по baseline; EV=BAC_task*percent/100; AC по actual cost; PV линейно по
рабочим координатам базового календаря до status date. Фиксированное смещение
status date переводится в смещение baseline. CPI/SPI — null при нулевом знаменателе.
Нет многофазного cost contour, time-phased actuals или reference comparison с Project.

Leveling — bounded greedy delay: выбирается перегруженная задача с допустимым
SNET/без ограничения, добавляется задержка и пересчитываются зависимости.
Вариант within_float не позволяет увеличить исходный срок проекта. Есть лимит
128 попыток; нерешённый preview нельзя применить. Нет split/resource replacement,
приоритетного оптимума и межпроектного выравнивания. Применение — одна Undo-группа.
