# Архитектура 0.9 Alpha

Модульный Rust проект: plan-core (домен/календарь/ресурсы/management/leveling/обмен),
plan-cli (web assets, local/SQLite workspace, API, auth, sync и launcher).
Core не читает сеть/SQL/системное время; auth/store находятся в адаптере.

Native serde model + validated newtypes. JSON native envelope schema=1, старые TSV
мигрируют через legacy parser. Документ не содержит history/secrets. Workspace Editor
snapshot хранит состояние/историю/used IDs в SQLite WAL, с audit/receipts/publications.
CAS/idempotency выполняются в одной transaction. PostgreSQL остаётся target design,
а не вторым поддержанным backend. Native schema и OpenAPI включены в приложение.

Minute Timeline и обратный монотонный проход — SCHEDULING.md. Personal calendars
сейчас только capacity intersection; полноценная assignment time distribution не готова.
Management содержит mode policies, costs, baseline snapshots, actuals и EVM.
Leveling — отдельный bounded greedy preview, не resource optimizer.

OIDC adapter: discovery/JWKS, RS256 issuer/aud/exp/roles, PKCE browser client.
SPА access token в sessionStorage; не реализован целевой BFF/session refresh/keychain.
Org-scoped RBAC, без project ACL. Static token fallback только явно настроенный.
Server synchronous/HTTP/1.1; production Tokio/Axum cancellation/quotas/observability
и полноценная нагрузочная верификация ещё не выполнены.

Desktop — запуск локального сервера и браузера. Offline CLI pull/push передаёт native
JSON + revision с credentials только из env. Tauri, outbox/merge и signed packaging
остаются P6. Docker/CI подготовлены; фактическая проверка сейчас Linux, не все платформы.

## Native client 0.11

Optional native-client feature adds Tao/Wry window and mode chooser. Local mode
owns a separate workspace process from the same executable; random per-launch
token protects loopback API, SQLite remains the durable model. Dropping the session
kills/waits the owned process. Remote view loads corporate HTTPS without local
filesystem/process IPC; only Home is exposed. Settings keep only server URL.
Window mode state, backend state and per-origin WebKit/WebView2 storage are distinct.
See docs/NATIVE_CLIENT.md for lifecycle and remaining offline-sync work.
