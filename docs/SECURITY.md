# Безопасность 0.11.1 Alpha

Workspace поддерживает scoped organization roles, static tokens или OIDC RS256.
Без auth config loopback работает как dev administrator; это удобство локальной
разработки, не разделение пользователей. Внешняя привязка требует auth и public
origin, удалённый origin — HTTPS. TLS завершает reverse proxy.

Ручной синхронный HTTP адаптер проверяет Host/Origin, лимиты тела/заголовков,
таймауты и неоднозначные критические headers. CSP не разрешает inline scripts;
пользовательские строки выводятся через textContent. Расчётные лимиты ограничивают
модель и горизонт. SQLite CAS, receipt и audit фиксируются одной транзакцией.

OIDC проверяет signature/iss/aud/exp/nbf, поддерживает JWKS refresh и PKCE S256.
Access token находится в sessionStorage: при XSS он доступен скрипту. Нет BFF,
refresh token lifecycle, federated logout, rate limiting, project ACL, resource
identity mapping, tamper-evident audit и production security review. Подробности
и доверенные metadata endpoints: AUTH.md. Тестовый loopback OIDC flag не включать
для production provider. Секреты храните вне исходников и резервных копий проекта.

SQLite содержит данные проектов, историю, публикации, табели и аудит в открытом
виде; задайте права каталога, шифрование диска и backup retention. Команда backup
проверена восстановлением; системный DR plan и внешние резервные копии не настроены.
Сервис обслуживает запросы синхронно и может задерживать остальные запросы на время
дорогого расчёта/чтения. Не считать alpha готовой к многопользовательской production
эксплуатации без hardening и проверки нагрузки.

Зависимости зафиксированы Cargo.lock, inventory в DEPENDENCIES.csv. Автоматический
security advisory scan в этой среде не выполнен. После создания публичного
репозитория сопровождающие должны определить приватный канал сообщений об уязвимостях.

Native client: local child requires a random per-session API token and exact
loopback origin. Home/Quit drops the owned process. Server-supplied content has
no native filesystem/process commands; native IPC permits only Home in that view.
Settings retain server URL only; webview profiles may retain corporate cookies.
No keychain, crash orphan recovery, single-instance guard or real-tenant native
SSO acceptance yet. Native code does not disable the web engine sandbox.
