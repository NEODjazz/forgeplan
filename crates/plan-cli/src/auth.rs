//! RS256 OIDC access-token verification with discovery and a bounded JWKS cache.
use jsonwebtoken::{decode, decode_header, jwk::JwkSet, Algorithm, DecodingKey, Validation};
use serde_json::{json, Value};
use std::{
    env,
    io::Read,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
type Result<T> = std::result::Result<T, String>;
struct Provider {
    issuer: String,
    audience: String,
    organization: String,
    client_id: String,
    authorization: String,
    token: String,
    jwks: String,
    keys: JwkSet,
    updated: Instant,
    client: reqwest::blocking::Client,
}
static PROVIDER: OnceLock<Result<Mutex<Provider>>> = OnceLock::new();
pub fn configured() -> bool {
    env::var_os("FORGEPLAN_OIDC_ISSUER").is_some()
}
fn required(name: &str) -> Result<String> {
    env::var(name)
        .ok()
        .filter(|s| !s.is_empty())
        .ok_or(format!("Missing {name}"))
}
fn url(s: &str) -> Result<reqwest::Url> {
    let u = reqwest::Url::parse(s).map_err(|_| "Invalid OIDC URL")?;
    if !u.username().is_empty() || u.password().is_some() || u.fragment().is_some() {
        return Err("OIDC URLs cannot contain credentials/fragments".into());
    }
    let loopback = env::var("FORGEPLAN_OIDC_ALLOW_LOOPBACK").as_deref() == Ok("1")
        && u.scheme() == "http"
        && u.host_str() == Some("127.0.0.1");
    if u.scheme() != "https" && !loopback {
        return Err("OIDC endpoints require HTTPS (loopback HTTP is test-only)".into());
    }
    Ok(u)
}
fn read<T: serde::de::DeserializeOwned>(
    client: &reqwest::blocking::Client,
    target: &str,
) -> Result<T> {
    url(target)?;
    let response = client
        .get(target)
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|_| "OIDC metadata/JWKS request failed")?;
    let mut body = String::new();
    response
        .take(1_000_001)
        .read_to_string(&mut body)
        .map_err(|_| "Cannot read OIDC response")?;
    if body.len() > 1_000_000 {
        return Err("OIDC metadata exceeds 1 MB".into());
    }
    serde_json::from_str(&body).map_err(|_| "Invalid OIDC metadata/JWKS JSON".into())
}
fn provider() -> Result<&'static Mutex<Provider>> {
    PROVIDER
        .get_or_init(|| {
            let issuer = required("FORGEPLAN_OIDC_ISSUER")?;
            url(&issuer)?;
            let audience = required("FORGEPLAN_OIDC_AUDIENCE")?;
            let client_id = required("FORGEPLAN_OIDC_CLIENT_ID")?;
            let organization = required("FORGEPLAN_OIDC_ORGANIZATION")?;
            let client = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(5))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(|e| e.to_string())?;
            let metadata: Value = read(
                &client,
                &format!(
                    "{}/.well-known/openid-configuration",
                    issuer.trim_end_matches('/')
                ),
            )?;
            if metadata["issuer"].as_str() != Some(&issuer) {
                return Err("OIDC discovery issuer mismatch".into());
            }
            let field = |key: &str| {
                metadata[key]
                    .as_str()
                    .map(str::to_string)
                    .ok_or(format!("OIDC metadata missing {key}"))
            };
            let authorization = field("authorization_endpoint")?;
            let token = field("token_endpoint")?;
            let jwks = field("jwks_uri")?;
            url(&authorization)?;
            url(&token)?;
            url(&jwks)?;
            let keys = read(&client, &jwks)?;
            Ok(Mutex::new(Provider {
                issuer,
                audience,
                organization,
                client_id,
                authorization,
                token,
                jwks,
                keys,
                updated: Instant::now(),
                client,
            }))
        })
        .as_ref()
        .map_err(Clone::clone)
}
pub fn config() -> Result<Value> {
    if !configured() {
        return Ok(Value::Null);
    }
    let p = provider()?.lock().map_err(|_| "OIDC lock failed")?;
    Ok(
        json!({"issuer":p.issuer,"client_id":p.client_id,"organization":p.organization,"authorization_endpoint":p.authorization,"token_endpoint":p.token}),
    )
}
pub fn connect_origin() -> Option<String> {
    let v = config().ok()?;
    let u = reqwest::Url::parse(v["token_endpoint"].as_str()?).ok()?;
    Some(u.origin().ascii_serialization())
}
pub fn verify(token: &str) -> Result<(String, String, String)> {
    if token.len() > 32000 {
        return Err("Unauthorized".into());
    }
    let header = decode_header(token).map_err(|_| "Unauthorized")?;
    if header.alg != Algorithm::RS256 {
        return Err("Unauthorized".into());
    }
    let kid = header.kid.ok_or("Unauthorized")?;
    let mut p = provider()?.lock().map_err(|_| "OIDC lock failed")?;
    if p.updated.elapsed() > Duration::from_secs(300)
        || (p.keys.find(&kid).is_none() && p.updated.elapsed() > Duration::from_secs(1))
    {
        p.keys = read(&p.client, &p.jwks)?;
        p.updated = Instant::now();
    }
    let jwk = p.keys.find(&kid).ok_or("Unauthorized")?;
    if jwk
        .common
        .public_key_use
        .as_ref()
        .is_some_and(|usage| usage != &jsonwebtoken::jwk::PublicKeyUse::Signature)
        || jwk
            .common
            .key_algorithm
            .as_ref()
            .is_some_and(|alg| alg != &jsonwebtoken::jwk::KeyAlgorithm::RS256)
    {
        return Err("Unauthorized".into());
    }
    if jwk
        .common
        .key_operations
        .as_ref()
        .is_some_and(|ops| !ops.contains(&jsonwebtoken::jwk::KeyOperations::Verify))
    {
        return Err("Unauthorized".into());
    }
    let key = DecodingKey::from_jwk(jwk).map_err(|_| "Unauthorized")?;
    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_audience(&[&p.audience]);
    validation.set_issuer(&[&p.issuer]);
    validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
    validation.validate_nbf = true;
    validation.leeway = 30;
    let claims = decode::<Value>(token, &key, &validation)
        .map_err(|_| "Unauthorized")?
        .claims;
    let subject = claims["sub"]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 255)
        .ok_or("Unauthorized")?
        .to_string();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "Clock error")?
        .as_secs();
    if claims
        .get("iat")
        .is_some_and(|iat| iat.as_u64().is_none_or(|iat| iat > now + 30))
    {
        return Err("Unauthorized".into());
    }
    if claims
        .get("organization")
        .is_some_and(|o| o.as_str() != Some(&p.organization))
    {
        return Err("Forbidden: token organization mismatch".into());
    }
    let roles = claims["roles"]
        .as_array()
        .into_iter()
        .flatten()
        .chain(
            claims["realm_access"]["roles"]
                .as_array()
                .into_iter()
                .flatten(),
        )
        .chain(
            claims["resource_access"][&p.client_id]["roles"]
                .as_array()
                .into_iter()
                .flatten(),
        )
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();
    let role = [
        ("forgeplan_admin", "admin"),
        ("forgeplan_manager", "manager"),
        ("forgeplan_editor", "editor"),
        ("forgeplan_viewer", "viewer"),
    ]
    .iter()
    .find(|(r, _)| roles.contains(r))
    .map(|(_, role)| role.to_string())
    .ok_or("Forbidden: no ForgePlan role")?;
    Ok((subject, p.organization.clone(), role))
}
