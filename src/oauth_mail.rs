//! Microsoft 365 mailboxes without passwords: the server signs in as an app registered in
//! the organisation's Entra ID (client credentials) and uses the token for IMAP and SMTP
//! (XOAUTH2). Microsoft has switched off plain passwords for these protocols.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// An app registration with the permissions IMAP.AccessAsApp / SMTP.SendAsApp.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OAuthApp {
    /// Directory (tenant) ID.
    pub tenant: String,
    pub client_id: String,
    /// Empty when shown to a workplace; saving it empty keeps the old one.
    pub client_secret: String,
}

impl OAuthApp {
    pub fn is_set(&self) -> bool {
        !self.tenant.trim().is_empty() && !self.client_id.trim().is_empty()
    }
}

pub const OUTLOOK_SCOPE: &str = "https://outlook.office365.com/.default";

/// Fetches an access token for Exchange Online.
pub fn token(app: &OAuthApp) -> Result<String, String> {
    #[derive(Deserialize)]
    struct Reply {
        access_token: Option<String>,
        error_description: Option<String>,
    }
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .http_status_as_error(false)
        .build();
    let agent = ureq::Agent::new_with_config(config);
    let reply: Reply = agent
        .post(format!(
            "https://login.microsoftonline.com/{}/oauth2/v2.0/token",
            crate::server::url_encode(app.tenant.trim())
        ))
        .send_form([
            ("client_id", app.client_id.trim()),
            ("client_secret", app.client_secret.as_str()),
            ("scope", OUTLOOK_SCOPE),
            ("grant_type", "client_credentials"),
        ])
        .map_err(|e| format!("Microsoft nicht erreichbar ({e})."))?
        .body_mut()
        .read_json()
        .map_err(|e| format!("Antwort von Microsoft unlesbar: {e}"))?;
    match reply.access_token {
        Some(t) => Ok(t),
        None => Err(format!(
            "Microsoft 365 hat die Anmeldung abgelehnt: {}",
            reply.error_description.unwrap_or_default().lines().next().unwrap_or("")
        )),
    }
}

/// The XOAUTH2 sign-in string for IMAP (base64).
pub fn xoauth2(user: &str, token: &str) -> String {
    crate::pdf::encode_base64(format!("user={user}\x01auth=Bearer {token}\x01\x01").as_bytes())
}

#[cfg(test)]
mod tests {
    #[test]
    fn xoauth2_format() {
        let s = super::xoauth2("a@b.de", "T");
        assert_eq!(crate::pdf::decode_base64(&s), b"user=a@b.de\x01auth=Bearer T\x01\x01");
    }
}
