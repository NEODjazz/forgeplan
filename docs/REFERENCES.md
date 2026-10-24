# Исходные материалы и порядок проверки совместимости

Матрица предварительная. Следующие официальные страницы описывают функции;
страницы для старых редакций нельзя считать актуальной дорожной картой Microsoft.

- https://support.microsoft.com/en-us/project/manage-your-project-s-critical-path
- https://support.microsoft.com/en-us/project/distribute-project-work-evenly-level-resource-assignments
- https://support.microsoft.com/en-us/project/project-for-the-web-and-project-online-desktop-client
- https://support.microsoft.com/en-us/project/project-online-setup-sequence

Порядок differential testing: выбрать редакцию/сборку Project → создать
малый план с известными календарями и настройками → сохранить исходник и XML →
зафиксировать ожидаемые значения и timezone → импортировать → сравнить даты,
work, cost и поля с объявленной точностью → классифицировать расхождение.
Эталонные файлы Microsoft Project в текущий комплект не входят.
Пока этот корпус не создан, нельзя заявлять полную функциональную или файловую
совместимость. Не включать сторонние библиотеки MPP без license/security review.
