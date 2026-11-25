//! Discord `OAuth2` authorization-code flow with PKCE (S256), public client.
//!
//! - No client secret anywhere.
//! - Redirect is a one-shot loopback listener on 127.0.0.1 only.
//! - `state` and `iss` are validated; `access_denied` is handled.

use std::time::Duration;

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::RngCore;
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::time::timeout;

/// Discord OAuth endpoints.
pub const AUTHORIZE_URL: &str = "https://discord.com/oauth2/authorize";
pub const TOKEN_URL: &str = "https://discord.com/api/oauth2/token";
pub const REVOKE_URL: &str = "https://discord.com/api/oauth2/token/revoke";

/// Scopes requested: minimum needed (`identify`, `guilds`, `guilds.join`).
pub const SCOPES: &str = "identify guilds guilds.join";

/// OAuth configuration.
#[derive(Debug, Clone)]
pub struct OAuthConfig {
    /// Public client id from the Discord Developer Portal.
    pub client_id: String,
    /// Loopback redirect port (the listener binds this port).
    pub port: u16,
    /// Optional redirect URI override. Must match a URI registered in the
    /// Developer Portal exactly. `None` = `http://127.0.0.1:{port}/callback`.
    pub redirect_uri: Option<String>,
}

impl OAuthConfig {
    /// The effective redirect URI sent to Discord.
    #[must_use]
    pub fn redirect_uri(&self) -> String {
        self.redirect_uri.clone().unwrap_or_else(|| {
            format!("http://127.0.0.1:{}/callback", self.port)
        })
    }
}

/// Tokens returned by the token endpoint. Secrets stay wrapped.
#[derive(Debug, Clone)]
pub struct OAuthTokens {
    /// Access token.
    pub access_token: SecretString,
    /// Refresh token, if issued.
    pub refresh_token: Option<SecretString>,
    /// Seconds until expiry.
    pub expires_in: u64,
    /// Granted scopes.
    pub scope: String,
    /// Token type (always `Bearer`).
    pub token_type: String,
}

/// OAuth errors.
#[derive(Debug, Error)]
pub enum OAuthError {
    /// User declined or Discord returned an error.
    #[error("authorization denied: {0}")]
    Denied(String),
    /// Loopback listener failed.
    #[error("listener error: {0}")]
    Listener(String),
    /// Timed out waiting for the redirect.
    #[error("timed out waiting for authorization")]
    Timeout,
    /// Token exchange failed.
    #[error("token exchange failed: {0}")]
    Exchange(String),
    /// State/iss validation failed.
    #[error("invalid oauth response: {0}")]
    InvalidResponse(String),
}

struct Pkce {
    verifier: String,
    challenge: String,
    state: String,
}

fn pkce() -> Pkce {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let verifier = URL_SAFE_NO_PAD.encode(bytes);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let mut sb = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut sb);
    Pkce {
        verifier,
        challenge,
        state: URL_SAFE_NO_PAD.encode(sb),
    }
}

/// Build the authorization URL the user opens in their browser.
#[must_use]
pub fn authorize_url(cfg: &OAuthConfig, pkce_challenge: &str, state: &str) -> String {
    let redirect = cfg.redirect_uri();
    format!(
        "{AUTHORIZE_URL}?response_type=code&client_id={}&redirect_uri={}&scope={}&state={}&code_challenge={}&code_challenge_method=S256",
        cfg.client_id,
        urlencoding(&redirect),
        urlencoding(SCOPES),
        urlencoding(state),
        urlencoding(pkce_challenge),
    )
}

fn urlencoding(s: &str) -> String {
    // Minimal percent-encoding for the query values we produce.
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                use std::fmt::Write as _;
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}

/// Run the full interactive flow: returns the authorization URL to open and a
/// future resolving to tokens.
pub async fn run_flow(
    cfg: OAuthConfig,
) -> Result<
    (
        String,
        impl std::future::Future<Output = Result<OAuthTokens, OAuthError>>,
    ),
    OAuthError,
> {
    let pkce = pkce();
    let url = authorize_url(&cfg, &pkce.challenge, &pkce.state);
    let listener = TcpListener::bind(("127.0.0.1", cfg.port))
        .await
        .map_err(|e| OAuthError::Listener(e.to_string()))?;

    let future = async move {
        // Wait for the one-shot callback.
        let (mut stream, _) = timeout(Duration::from_secs(300), listener.accept())
            .await
            .map_err(|_| OAuthError::Timeout)?
            .map_err(|e| OAuthError::Listener(e.to_string()))?;

        let mut buf = vec![0u8; 8192];
        let n = timeout(Duration::from_secs(10), stream.read(&mut buf))
            .await
            .map_err(|_| OAuthError::Timeout)?
            .map_err(|e| OAuthError::Listener(e.to_string()))?;
        let request = String::from_utf8_lossy(&buf[..n]);
        let path = request
            .lines()
            .next()
            .and_then(|l| l.split_whitespace().nth(1))
            .unwrap_or("");

        // Parse query params.
        let query = path.split('?').nth(1).unwrap_or("");
        let params: std::collections::HashMap<String, String> = query
            .split('&')
            .filter_map(|kv| {
                let (k, v) = kv.split_once('=')?;
                Some((percent_decode(k), percent_decode(v)))
            })
            .collect();

        let code = params.get("code").cloned();
        let state = params.get("state").cloned();
        let error = params.get("error").cloned();
        let iss = params.get("iss").cloned();

        // Validate iss when present (Discord sends iss on errors).
        if let Some(iss) = iss
            && iss != "https://discord.com"
        {
            serve_page(&mut stream, 400, "invalid issuer").await;
            return Err(OAuthError::InvalidResponse("bad iss".into()));
        }

        match (error, code, state) {
            (Some(err), _, _) => {
                serve_page(
                    &mut stream,
                    200,
                    "authorization denied — you can close this tab",
                )
                .await;
                Err(OAuthError::Denied(err))
            }
            (None, Some(code), Some(st)) if st == pkce.state => {
                serve_page(&mut stream, 200, "authorized — you can close this tab").await;
                exchange_code(&cfg.client_id, &code, &pkce.verifier, &cfg.redirect_uri()).await
            }
            _ => {
                serve_page(&mut stream, 400, "invalid oauth response").await;
                Err(OAuthError::InvalidResponse(
                    "state mismatch or missing code".into(),
                ))
            }
        }
    };

    Ok((url, future))
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(if bytes[i] == b'+' { b' ' } else { bytes[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

async fn serve_page(stream: &mut tokio::net::TcpStream, _status: u16, msg: &str) {
    let body = format!(
        "<!doctype html><meta charset=utf-8><title>L</title>\
         <body style='background:#090909;color:#888;font-family:monospace;display:grid;place-items:center;height:100vh'>\
         <div>L / {msg}</div></body>"
    );
    let resp = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: text/html\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(resp.as_bytes()).await;
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: u64,
    scope: String,
    token_type: String,
}

/// Exchange the authorization code for tokens (public client: no secret).
pub async fn exchange_code(
    client_id: &str,
    code: &str,
    verifier: &str,
    redirect_uri: &str,
) -> Result<OAuthTokens, OAuthError> {
    let client = reqwest::Client::new();
    let resp = client
        .post(TOKEN_URL)
        .form(&[
            ("client_id", client_id),
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("code_verifier", verifier),
        ])
        .send()
        .await
        .map_err(|e| OAuthError::Exchange(e.to_string()))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(OAuthError::Exchange(format!("{status}: {text}")));
    }
    let t: TokenResponse = resp
        .json()
        .await
        .map_err(|e| OAuthError::Exchange(e.to_string()))?;
    Ok(OAuthTokens {
        access_token: SecretString::from(t.access_token),
        refresh_token: t.refresh_token.map(SecretString::from),
        expires_in: t.expires_in,
        scope: t.scope,
        token_type: t.token_type,
    })
}

/// Refresh an access token.
pub async fn refresh_token(
    client_id: &str,
    refresh: &SecretString,
) -> Result<OAuthTokens, OAuthError> {
    let client = reqwest::Client::new();
    let resp = client
        .post(TOKEN_URL)
        .form(&[
            ("client_id", client_id),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh.expose_secret()),
        ])
        .send()
        .await
        .map_err(|e| OAuthError::Exchange(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(OAuthError::Exchange(resp.status().to_string()));
    }
    let t: TokenResponse = resp
        .json()
        .await
        .map_err(|e| OAuthError::Exchange(e.to_string()))?;
    Ok(OAuthTokens {
        access_token: SecretString::from(t.access_token),
        refresh_token: t.refresh_token.map(SecretString::from),
        expires_in: t.expires_in,
        scope: t.scope,
        token_type: t.token_type,
    })
}

/// Revoke a token (best-effort).
pub async fn revoke_token(client_id: &str, token: &SecretString) {
    let client = reqwest::Client::new();
    let _ = client
        .post(REVOKE_URL)
        .form(&[
            ("client_id", client_id),
            ("token", token.expose_secret()),
            ("token_type_hint", "access_token"),
        ])
        .send()
        .await;
}
