//! Workplace side of the Microsoft 365 sign-in: the OAuth 2.0 authorization code flow
//! with PKCE in the system browser, as recommended for desktop apps. Microsoft returns
//! to a short-lived address on this computer (http://localhost:<port>/).

use crate::server::url_encode;
use crate::sso::{SsoInfo, b64url_encode};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// How long to wait for the person to finish signing in.
const WAIT: Duration = Duration::from_secs(300);

/// Proof that Microsoft signed someone in, for the OpenKontor server to check.
pub struct SsoResult {
    pub id_token: String,
    pub nonce: String,
}

fn random_b64(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf).expect("Zufallszahlen nicht verfügbar");
    b64url_encode(&buf)
}

#[derive(Deserialize)]
struct ServerInfo {
    #[serde(default)]
    sso: Option<SsoInfo>,
}

/// Asks the OpenKontor server whether (and how) Microsoft 365 sign-in is set up.
pub fn fetch_info(base: &str) -> Result<Option<SsoInfo>, String> {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(8)))
        .build();
    let agent = ureq::Agent::new_with_config(config);
    let info: ServerInfo = agent
        .get(format!("{base}/api/info"))
        .call()
        .map_err(|e| format!("Server nicht erreichbar ({e})."))?
        .body_mut()
        .read_json()
        .map_err(|e| format!("Antwort unlesbar: {e}"))?;
    Ok(info.sso)
}

/// The query parameters of a URL path like "/?code=…&state=…".
fn query_params(url: &str) -> Vec<(String, String)> {
    let query = url.split_once('?').map(|(_, q)| q).unwrap_or("");
    query
        .split('&')
        .filter_map(|kv| kv.split_once('='))
        .map(|(k, v)| (k.to_string(), percent_decode(v)))
        .collect()
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => {
                if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                    out.push(v);
                    i += 2;
                } else {
                    out.push(b'%');
                }
            }
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn page(title: &str, text: &str) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    let html = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>OpenKontor</title></head>\
         <body style=\"font-family:sans-serif;background:#152e47;color:#fff;display:flex;\
         align-items:center;justify-content:center;height:100vh;margin:0\"><div style=\"text-align:center\">\
         <h2>{title}</h2><p>{text}</p></div></body></html>"
    );
    tiny_http::Response::from_data(html.into_bytes()).with_header(
        tiny_http::Header::from_bytes("Content-Type", "text/html; charset=utf-8").unwrap(),
    )
}

/// The address the browser opens to sign in.
pub fn authorize_url(info: &SsoInfo, redirect: &str, state: &str, nonce: &str, challenge: &str) -> String {
    format!(
        "{}/{}/oauth2/v2.0/authorize?client_id={}&response_type=code&response_mode=query\
         &redirect_uri={}&scope={}&state={}&nonce={}&code_challenge={}&code_challenge_method=S256\
         &prompt=select_account",
        info.authority.trim_end_matches('/'),
        url_encode(&info.tenant),
        url_encode(&info.client_id),
        url_encode(redirect),
        url_encode("openid profile email"),
        url_encode(state),
        url_encode(nonce),
        url_encode(challenge),
    )
}

/// Runs the whole browser sign-in. Blocks until done, cancelled or timed out,
/// so call it from a background thread.
pub fn browser_login(info: &SsoInfo, cancel: Arc<AtomicBool>) -> Result<SsoResult, String> {
    let t = |de: &'static str| crate::i18n::t(de);
    let listener = tiny_http::Server::http("127.0.0.1:0")
        .map_err(|e| format!("Lokale Rückmeldeadresse nicht verfügbar: {e}"))?;
    let port = listener
        .server_addr()
        .to_ip()
        .map(|a| a.port())
        .ok_or("Lokale Rückmeldeadresse nicht verfügbar.")?;
    let redirect = format!("http://localhost:{port}/");
    let verifier = random_b64(32);
    let challenge = b64url_encode(&Sha256::digest(verifier.as_bytes()));
    let state = random_b64(16);
    let nonce = random_b64(16);

    crate::export::open_url(&authorize_url(info, &redirect, &state, &nonce, &challenge));

    let started = Instant::now();
    let code = loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(t("Anmeldung abgebrochen.").into());
        }
        if started.elapsed() > WAIT {
            return Err(t("Die Anmeldung im Browser hat zu lange gedauert. Bitte erneut versuchen.").into());
        }
        let Ok(Some(request)) = listener.recv_timeout(Duration::from_millis(300)) else {
            continue;
        };
        let params = query_params(request.url());
        let get = |k: &str| params.iter().find(|(key, _)| key == k).map(|(_, v)| v.clone());
        if let Some(err) = get("error") {
            let detail = get("error_description").unwrap_or(err);
            let _ = request.respond(page(t("Anmeldung fehlgeschlagen"), &detail));
            return Err(format!("{}: {detail}", t("Microsoft hat die Anmeldung abgelehnt")));
        }
        match (get("code"), get("state")) {
            (Some(code), Some(s)) if s == state => {
                let _ = request.respond(page(
                    t("Anmeldung abgeschlossen"),
                    t("Sie können dieses Fenster schließen und zu OpenKontor zurückkehren."),
                ));
                break code;
            }
            // Favicon requests and the like: answer and keep waiting.
            _ => {
                let _ = request.respond(tiny_http::Response::empty(404));
            }
        }
    };

    // Exchange the code for the ID token (public client: no secret, the PKCE verifier proves it is us).
    #[derive(Deserialize)]
    struct TokenReply {
        id_token: Option<String>,
        error_description: Option<String>,
    }
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .http_status_as_error(false)
        .build();
    let agent = ureq::Agent::new_with_config(config);
    let reply: TokenReply = agent
        .post(format!(
            "{}/{}/oauth2/v2.0/token",
            info.authority.trim_end_matches('/'),
            url_encode(&info.tenant)
        ))
        .send_form([
            ("client_id", info.client_id.as_str()),
            ("grant_type", "authorization_code"),
            ("code", code.as_str()),
            ("redirect_uri", redirect.as_str()),
            ("code_verifier", verifier.as_str()),
            ("scope", "openid profile email"),
        ])
        .map_err(|e| format!("Microsoft nicht erreichbar ({e})."))?
        .body_mut()
        .read_json()
        .map_err(|e| format!("Antwort von Microsoft unlesbar: {e}"))?;
    match reply.id_token {
        Some(id_token) => Ok(SsoResult { id_token, nonce }),
        None => Err(reply
            .error_description
            .unwrap_or_else(|| t("Microsoft hat keinen Anmeldenachweis geliefert.").into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authorize_url_has_pkce_and_redirect() {
        let info = SsoInfo {
            tenant: "11111111-2222-3333-4444-555555555555".into(),
            client_id: "app".into(),
            authority: "https://login.microsoftonline.com".into(),
        };
        let url = authorize_url(&info, "http://localhost:5555/", "st", "no", "ch");
        assert!(url.starts_with(
            "https://login.microsoftonline.com/11111111-2222-3333-4444-555555555555/oauth2/v2.0/authorize?"
        ));
        assert!(url.contains("redirect_uri=http%3A%2F%2Flocalhost%3A5555%2F"));
        assert!(url.contains("code_challenge=ch&code_challenge_method=S256"));
        assert!(url.contains("scope=openid%20profile%20email"));
    }

    #[test]
    fn reads_query_parameters() {
        let p = query_params("/?code=a%2Bb&state=x+y");
        assert_eq!(p, vec![("code".into(), "a+b".into()), ("state".into(), "x y".into())]);
    }
}
