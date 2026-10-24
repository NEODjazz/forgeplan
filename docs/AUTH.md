# OIDC / SSO в alpha

Используется Authorization Code + S256 PKCE в браузере и RS256 JWT verification
на API. Browser получает access token; ID token не используется как источник прав.
Это SPA/sessionStorage, а не BFF с HttpOnly cookie. Refresh token и federated logout
не реализованы; после истечения access token нужен новый вход. Локальный logout
удаляет token/PKCE state. UI видит только подтверждённые API роли.

## Конфигурация

- FORGEPLAN_OIDC_ISSUER — точный issuer, например tenant/realm-specific URL.
- FORGEPLAN_OIDC_AUDIENCE — ожидаемый audience API access token.
- FORGEPLAN_OIDC_CLIENT_ID — public browser client.
- FORGEPLAN_OIDC_ORGANIZATION — организация рабочего пространства.
- FORGEPLAN_PUBLIC_ORIGIN — origin приложения за reverse proxy, если используется.

IdP должен разрешить redirect URI `PUBLIC_ORIGIN/`, public client Code flow с PKCE
и CORS token endpoint для origin приложения. API access token должен иметь нужный
`aud`; это может требовать audience mapper/конфигурации application ID URI.

Discovery issuer проверяется точно. Metadata/token/JWKS URLs требуют HTTPS,
credentials/fragment запрещены, редиректы metadata не выполняются. Ответы metadata
ограничены 1 MB и timeout 5 секунд. Кэш JWKS 300 секунд, unknown kid вызывает
ограниченное обновление не чаще раза в секунду. Проверяются alg=RS256, signature,
issuer/aud, обязательные exp/sub, nbf и future iat при наличии; leeway 30 секунд.
JWK с явно неподходящим use/alg/key_ops не принимается.

Роли: forgeplan_viewer/editor/manager/admin в claims.roles,
realm_access.roles или resource_access[CLIENT_ID].roles. Непрефиксная admin
не даёт прав; JWT без ForgePlan role получает 403. Подтверждённая organization
сравнивается с X-Organization; optional claim organization также проверяется.

Для Keycloak это realm/client roles + audience mapper; для Entra — app roles
с теми же values, tenant-specific issuer и API audience. Эти реальные интеграции
не выполнялись: проверка проведена на локальном RSA mock IdP, включая настоящий
browser PKCE redirect/exchange и JWKS rotation. Access token должен быть JWT,
opaque tokens не поддерживаются. HTTPS internal CA configuration отдельно не задана.

FORGEPLAN_OIDC_ALLOW_LOOPBACK=1 разрешает HTTP только 127.0.0.1 для mock-тестов.
Не используйте этот режим как поддержку insecure remote issuer.
Static FORGEPLAN_IDENTITIES можно оставить как явно настроенный API-token путь;
если любая auth-конфигурация задана, нет fallback в неавторизованный dev-admin.

Источник протокола: https://openid.net/specs/openid-connect-discovery-1_0.html
и https://www.rfc-editor.org/rfc/rfc7636.
