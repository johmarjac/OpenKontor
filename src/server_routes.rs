//! The HTTP routes of the server, behind sign-in.

use super::*;

pub(super) fn route(state: &mut State, method: &Method, path: &str, query: &str, body: &str, raw: &[u8], token: Option<&str>) -> Reply {
    match (method, path) {
        (&Method::Get, "/" | "/app" | "/index.html") => return text_response(crate::webapp::PAGE, "text/html; charset=utf-8"),
        (&Method::Get, "/manifest.json") => return text_response(crate::webapp::MANIFEST, "application/manifest+json"),
        (&Method::Get, "/icon.svg") => return text_response(crate::webapp::ICON, "image/svg+xml"),
        (&Method::Get, "/api/info") => {
            let sso = state.sso.as_ref().map(|v| v.cfg.info());
            return json_response(
                200,
                &serde_json::json!({ "name": state.name, "app": "OpenKontor", "sso": sso, "tenants": tenant_list(state), "version": env!("CARGO_PKG_VERSION") }),
            );
        }
        (&Method::Post, "/api/login/sso") => return sso_login(state, body),
        (&Method::Post, "/api/login") => return login(state, body),
        (&Method::Post, "/api/login/token") => return token_login(state, body),
        (&Method::Post, "/api/logout") => return forget(state, body),
        _ => {}
    }

    let Some(Session { user, role, tenant: ti }) = token.and_then(|t| state.sessions.get(t).cloned()) else {
        return error(401, "Bitte neu anmelden.");
    };
    let need = |ok: bool, msg: &str| if ok { None } else { Some(error(403, msg)) };
    let admin_only = "Nur Administratoren dürfen das.";

    match (method, path) {
        (&Method::Get, "/api/state") => {
            let since: u64 = query_value(query, "since").and_then(|v| v.parse().ok()).unwrap_or(0);
            let t = &state.tenants[ti];
            let rev = t.stored.revision;
            let data = (since != rev).then(|| t.stored.data.clone());
            json_response(200, &StateReply { revision: rev, data })
        }
        (&Method::Post, "/api/op") => {
            let op: Op = match serde_json::from_str(body) {
                Ok(op) => op,
                Err(e) => return error(400, &format!("Ungültige Änderung: {e}")),
            };
            if !role.can_write() {
                return error(403, "Sie haben nur Leserechte.");
            }
            if !role.allows(op.area()) {
                return error(403, "Dafür fehlen Ihnen die Rechte. Bitte wenden Sie sich an den Administrator.");
            }
            if op.admin_only() && role != Role::Admin {
                return error(403, admin_only);
            }
            let t = &mut state.tenants[ti];
            let deleted_receipt = match op {
                Op::DeleteReceipt(id) => t.file_key(id).map(|k| (id, k)),
                _ => None,
            };
            let mut copy = t.stored.data.clone();
            match copy.apply(op, &user) {
                Ok(id) => {
                    if let Err(e) = t.commit(copy) {
                        return error(500, &e);
                    }
                    if let Some((rid, key)) = deleted_receipt {
                        t.delete_file(rid, &key);
                    }
                    t.op_reply(id)
                }
                Err(e) => error(409, &e),
            }
        }
        (&Method::Post, "/api/receipts") => {
            if let Some(r) = need(role.allows(OpArea::Receipts), "Sie haben nur Leserechte.") {
                return r;
            }
            let name = query_value(query, "name").unwrap_or_else(|| "beleg.pdf".into());
            let source = format!("Hochgeladen von {user}");
            let t = &mut state.tenants[ti];
            match store_receipt(t, &name, raw, &source, &user) {
                Ok(id) => t.op_reply(Some(id)),
                Err(e) => error(409, &e),
            }
        }
        (&Method::Get, p) if p.starts_with("/api/receipts/") && p.ends_with("/file") => {
            let id: u64 = p["/api/receipts/".len()..p.len() - "/file".len()].parse().unwrap_or(0);
            let t = &mut state.tenants[ti];
            let Some(r) = t.stored.data.receipts.iter().find(|r| r.id == id).cloned() else {
                return error(404, "Beleg nicht gefunden.");
            };
            match t.get_file(id) {
                Ok(bytes) => Response::from_data(bytes)
                    .with_header(Header::from_bytes("Content-Type", content_type_for(&r.file_name)).unwrap()),
                Err(e) => error(404, &e),
            }
        }
        (&Method::Post, p) if p.starts_with("/api/receipts/") && p.ends_with("/recognize") => {
            if let Some(r) = need(role.allows(OpArea::Receipts), "Sie haben nur Leserechte.") {
                return r;
            }
            let id: u64 = p["/api/receipts/".len()..p.len() - "/recognize".len()].parse().unwrap_or(0);
            let t = &mut state.tenants[ti];
            let Some(mut r) = t.stored.data.receipts.iter().find(|r| r.id == id).cloned() else {
                return error(404, "Beleg nicht gefunden.");
            };
            let bytes = match t.get_file(id) {
                Ok(b) => b,
                Err(e) => return error(404, &e),
            };
            r.category.clear();
            crate::inbox::recognize(&t.stored.data, &mut r, &bytes);
            let mut copy = t.stored.data.clone();
            if let Err(e) = copy.apply(Op::SaveReceipt(Box::new(r)), &user) {
                return error(409, &e);
            }
            if let Err(e) = t.commit(copy) {
                return error(500, &e);
            }
            t.op_reply(Some(id))
        }
        (&Method::Get, "/api/inbox/settings") => {
            if let Some(r) = need(role == Role::Admin, "Nur Administratoren dürfen das Sammelpostfach einrichten.") {
                return r;
            }
            let mut settings = state.tenants[ti].stored.inbox.clone();
            let password_set = !settings.password.is_empty() || !settings.microsoft.client_secret.is_empty();
            settings.password.clear();
            settings.microsoft.client_secret.clear();
            json_response(200, &serde_json::json!({ "settings": settings, "password_set": password_set }))
        }
        (&Method::Post, "/api/inbox/settings") => {
            if let Some(r) = need(role == Role::Admin, "Nur Administratoren dürfen das Sammelpostfach einrichten.") {
                return r;
            }
            let Ok(mut settings) = serde_json::from_str::<crate::imap::InboxSettings>(body) else {
                return error(400, "Ungültige Anfrage.");
            };
            let t = &mut state.tenants[ti];
            if settings.password.is_empty() {
                settings.password = t.stored.inbox.password.clone();
            }
            if settings.microsoft.client_secret.is_empty() {
                settings.microsoft.client_secret = t.stored.inbox.microsoft.client_secret.clone();
            }
            settings.last_check = t.stored.inbox.last_check.clone();
            settings.last_result = t.stored.inbox.last_result.clone();
            let before = std::mem::replace(&mut t.stored.inbox, settings);
            match t.save(Change::Inbox) {
                Ok(()) => {
                    *t.inbox_shared.lock().unwrap() = t.stored.inbox.clone();
                    json_response(200, &serde_json::json!({ "ok": true }))
                }
                Err(e) => {
                    t.stored.inbox = before;
                    error(500, &format!("Speichern fehlgeschlagen: {e}"))
                }
            }
        }
        (&Method::Post, "/api/inbox/check") => {
            if let Some(r) = need(role.allows(OpArea::Receipts), "Sie haben nur Leserechte.") {
                return r;
            }
            state.tenants[ti].inbox_now.store(true, Ordering::SeqCst);
            json_response(200, &serde_json::json!({ "ok": true }))
        }
        (&Method::Get, "/api/mail/settings") => {
            if let Some(r) = need(role == Role::Admin, "Nur Administratoren dürfen den E-Mail-Versand einrichten.") {
                return r;
            }
            let mut settings = state.tenants[ti].stored.mail.clone();
            let password_set = !settings.password.is_empty() || !settings.microsoft.client_secret.is_empty();
            settings.password.clear();
            settings.microsoft.client_secret.clear();
            json_response(200, &MailSettingsReply { settings, password_set })
        }
        (&Method::Post, "/api/mail/settings") => {
            if let Some(r) = need(role == Role::Admin, "Nur Administratoren dürfen den E-Mail-Versand einrichten.") {
                return r;
            }
            let Ok(mut settings) = serde_json::from_str::<crate::mail::MailSettings>(body) else {
                return error(400, "Ungültige Anfrage.");
            };
            let t = &mut state.tenants[ti];
            if settings.password.is_empty() {
                settings.password = t.stored.mail.password.clone();
            }
            if settings.microsoft.client_secret.is_empty() {
                settings.microsoft.client_secret = t.stored.mail.microsoft.client_secret.clone();
            }
            let before = std::mem::replace(&mut t.stored.mail, settings);
            match t.save(Change::Mail) {
                Ok(()) => json_response(200, &serde_json::json!({ "ok": true })),
                Err(e) => {
                    t.stored.mail = before;
                    error(500, &format!("Speichern fehlgeschlagen: {e}"))
                }
            }
        }
        (&Method::Post, "/api/mail/test") => {
            if let Some(r) = need(role == Role::Admin, "Nur Administratoren dürfen den E-Mail-Versand einrichten.") {
                return r;
            }
            let Ok(req) = serde_json::from_str::<crate::mail::TestRequest>(body) else {
                return error(400, "Ungültige Anfrage.");
            };
            let text = format!(
                "Diese Test-E-Mail wurde vom OpenKontor-Server „{}“ verschickt. Der E-Mail-Versand funktioniert.",
                state.name
            );
            match crate::mail::send(&state.tenants[ti].stored.mail, &req.to, "", "OpenKontor: Test-E-Mail", &text, vec![]) {
                Ok(()) => json_response(200, &serde_json::json!({ "ok": true })),
                Err(e) => error(502, &e),
            }
        }
        (&Method::Post, "/api/mail/send") => {
            if let Some(r) = need(role.allows(OpArea::Sales), "Dafür fehlen Ihnen die Rechte.") {
                return r;
            }
            let req: crate::mail::SendRequest = match serde_json::from_str(body) {
                Ok(r) => r,
                Err(e) => return error(400, &format!("Ungültige Anfrage: {e}")),
            };
            let t = &mut state.tenants[ti];
            let mut copy = t.stored.data.clone();
            if let Err(e) = crate::mail::send_document(&mut copy, &t.stored.mail, &req, &user) {
                return error(502, &e);
            }
            // The e-mail is out; now keep the record of it.
            if let Err(e) = t.commit(copy) {
                return error(500, &format!("E-Mail gesendet, aber der Eintrag konnte nicht gespeichert werden: {e}"));
            }
            t.op_reply(Some(req.doc_id))
        }
        (&Method::Get, "/api/export/datev") => {
            if let Some(r) = need(role.allows(OpArea::Accounting), "Dafür fehlen Ihnen die Rechte.") {
                return r;
            }
            let parse = |k| query_value(query, k).and_then(|v| crate::model::parse_date(&v));
            let (Some(from), Some(to)) = (parse("from"), parse("to")) else {
                return error(400, "Bitte Zeitraum angeben.");
            };
            let t = &mut state.tenants[ti];
            let data = t.stored.data.clone();
            let zip = crate::exports::datev_package(&data, from, to, &mut |id| t.get_file(id).ok());
            Response::from_data(zip).with_header(Header::from_bytes("Content-Type", "application/zip").unwrap())
        }
        (&Method::Post, "/api/password") => {
            let Ok(req) = serde_json::from_str::<PasswordRequest>(body) else {
                return error(400, "Ungültige Anfrage.");
            };
            if req.new.len() < 6 {
                return error(400, "Das neue Passwort braucht mindestens 6 Zeichen.");
            }
            let t = &mut state.tenants[ti];
            let before = t.stored.users.clone();
            let Some(u) = t.stored.users.iter_mut().find(|u| u.name == user) else {
                return error(404, "Benutzer nicht gefunden.");
            };
            if u.hash.is_empty() {
                return error(409, "Dieses Konto meldet sich mit Microsoft 365 an.");
            }
            if hash_password(&u.salt, &req.old) != u.hash {
                return error(403, "Das bisherige Passwort stimmt nicht.");
            }
            let totp = std::mem::take(&mut u.totp);
            *u = make_user(&u.name.clone(), u.role, &req.new);
            u.totp = totp;
            t.save_users(before)
        }
        (&Method::Get, "/api/totp") => {
            let on = state.tenants[ti].stored.users.iter().any(|u| u.name == user && !u.totp.is_empty());
            json_response(200, &serde_json::json!({ "enabled": on }))
        }
        (&Method::Post, "/api/totp/start") => {
            let secret = crate::totp::new_secret();
            let issuer = format!("OpenKontor {}", state.tenants[ti].name());
            let uri = crate::totp::uri(&secret, &user, &issuer);
            state.totp_pending.insert((ti, user.clone()), secret.clone());
            json_response(200, &TotpStart { secret, uri })
        }
        (&Method::Post, "/api/totp/enable") => {
            #[derive(Deserialize)]
            struct Req {
                code: String,
            }
            let Ok(req) = serde_json::from_str::<Req>(body) else {
                return error(400, "Ungültige Anfrage.");
            };
            let Some(secret) = state.totp_pending.get(&(ti, user.clone())).cloned() else {
                return error(409, "Bitte zuerst die Einrichtung starten.");
            };
            let Some(step) = crate::totp::verify(&secret, &req.code, crate::totp::current_step()) else {
                return error(403, "Der Bestätigungscode stimmt nicht. Bitte die Uhrzeit des Handys prüfen.");
            };
            state.totp_pending.remove(&(ti, user.clone()));
            state.totp_used.insert((ti, user.clone()), step);
            let t = &mut state.tenants[ti];
            let before = t.stored.users.clone();
            let Some(u) = t.stored.users.iter_mut().find(|u| u.name == user) else {
                return error(404, "Benutzer nicht gefunden.");
            };
            u.totp = secret;
            t.save_users(before)
        }
        (&Method::Post, "/api/totp/disable") => {
            #[derive(Deserialize)]
            struct Req {
                #[serde(default)]
                user: String,
                #[serde(default)]
                password: String,
            }
            let Ok(req) = serde_json::from_str::<Req>(body) else {
                return error(400, "Ungültige Anfrage.");
            };
            // Administrators can switch it off for someone who lost the phone.
            let target = if req.user.is_empty() { user.clone() } else { req.user.clone() };
            let t = &mut state.tenants[ti];
            let before = t.stored.users.clone();
            let Some(u) = t.stored.users.iter_mut().find(|u| u.name.eq_ignore_ascii_case(&target)) else {
                return error(404, "Benutzer nicht gefunden.");
            };
            if u.name == user {
                if hash_password(&u.salt, &req.password) != u.hash {
                    return error(403, "Das Passwort stimmt nicht.");
                }
            } else if role != Role::Admin {
                return error(403, admin_only);
            }
            u.totp.clear();
            t.save_users(before)
        }
        (&Method::Get, "/api/users") => {
            if let Some(r) = need(role == Role::Admin, "Nur Administratoren dürfen Benutzer verwalten.") {
                return r;
            }
            let list: Vec<UserInfo> = state.tenants[ti]
                .stored
                .users
                .iter()
                .map(|u| UserInfo { name: u.name.clone(), role: u.role, sso_only: u.hash.is_empty(), totp: !u.totp.is_empty() })
                .collect();
            json_response(200, &list)
        }
        (&Method::Post, "/api/users") => {
            if let Some(r) = need(role == Role::Admin, "Nur Administratoren dürfen Benutzer verwalten.") {
                return r;
            }
            let Ok(req) = serde_json::from_str::<SaveUserRequest>(body) else {
                return error(400, "Ungültige Anfrage.");
            };
            save_user(state, ti, req)
        }
        (&Method::Delete, p) if p.starts_with("/api/users/") => {
            if let Some(r) = need(role == Role::Admin, "Nur Administratoren dürfen Benutzer verwalten.") {
                return r;
            }
            let name = url_decode(&p["/api/users/".len()..]);
            if name.eq_ignore_ascii_case(&user) {
                return error(409, "Sie können sich nicht selbst löschen.");
            }
            let t = &mut state.tenants[ti];
            let before = t.stored.users.clone();
            t.stored.users.retain(|u| !u.name.eq_ignore_ascii_case(&name));
            state.sessions.retain(|_, s| !(s.tenant == ti && s.user.eq_ignore_ascii_case(&name)));
            state.tenants[ti].save_users(before)
        }
        (&Method::Get, "/api/files/settings") => {
            if let Some(r) = need(role == Role::Admin, "Nur Administratoren dürfen die Belegablage einrichten.") {
                return r;
            }
            let t = &state.tenants[ti];
            let s = &t.stored.files;
            let default_dir = t.dir.join("Belege").display().to_string();
            json_response(200, &serde_json::json!({ "settings": s.without_secrets(), "secret_set": s.has_secret(), "default_dir": default_dir }))
        }
        (&Method::Post, "/api/files/settings") => {
            if let Some(r) = need(role == Role::Admin, "Nur Administratoren dürfen die Belegablage einrichten.") {
                return r;
            }
            #[derive(Deserialize)]
            struct Req {
                settings: crate::filestore::FileSettings,
                #[serde(default)]
                copy: bool,
                #[serde(default)]
                test_only: bool,
            }
            let Ok(mut req) = serde_json::from_str::<Req>(body) else {
                return error(400, "Ungültige Anfrage.");
            };
            let t = &mut state.tenants[ti];
            req.settings.keep_secrets(&t.stored.files);
            let mut new = match crate::filestore::open(&req.settings, &t.dir) {
                Ok(b) => b,
                Err(e) => return error(400, &e),
            };
            if let Some(b) = &mut new
                && let Err(e) = crate::filestore::check(b.as_mut())
            {
                return error(502, &format!("Die Ablage funktioniert nicht: {e}"));
            }
            if req.test_only {
                return json_response(200, &serde_json::json!({ "message": "Die Ablage funktioniert." }));
            }
            // Bring the files stored so far over to the new place.
            let mut copied = 0;
            if req.copy {
                let ids: Vec<u64> = t.stored.data.receipts.iter().map(|r| r.id).collect();
                for id in ids {
                    let Ok(bytes) = t.get_file(id) else { continue };
                    let key = t.file_key(id).unwrap_or_default();
                    let r = match &mut new {
                        Some(b) => b.put(&key, &bytes),
                        None => {
                            let _ = t.store.delete_file(id);
                            t.store.save_file(id, &bytes)
                        }
                    };
                    if let Err(e) = r {
                        return error(502, &format!("Übertragen abgebrochen nach {copied} Belegen: {e}"));
                    }
                    copied += 1;
                }
            }
            let before = std::mem::replace(&mut t.stored.files, req.settings);
            if let Err(e) = t.save(Change::Files) {
                t.stored.files = before;
                return error(500, &format!("Speichern fehlgeschlagen: {e}"));
            }
            t.files = new;
            let msg = if req.copy { format!("Belegablage gespeichert, {copied} Belege übertragen.") } else { "Belegablage gespeichert.".to_string() };
            json_response(200, &serde_json::json!({ "message": msg }))
        }
        (&Method::Get, "/api/tenants") => json_response(200, &tenant_list(state)),
        (&Method::Post, "/api/tenants") => {
            if let Some(r) = need(role == Role::Admin, admin_only) {
                return r;
            }
            #[derive(Deserialize)]
            struct Req {
                name: String,
            }
            let Ok(req) = serde_json::from_str::<Req>(body) else {
                return error(400, "Ungültige Anfrage.");
            };
            create_tenant(state, ti, &user, req.name.trim())
        }
        _ => error(404, "Unbekannte Adresse."),
    }
}

fn tenant_list(state: &State) -> Vec<TenantInfo> {
    state.tenants.iter().map(|t| TenantInfo { id: t.id.clone(), name: t.name() }).collect()
}

fn save_user(state: &mut State, ti: usize, req: SaveUserRequest) -> Reply {
    let name = req.name.trim().to_string();
    if name.is_empty() {
        return error(400, "Bitte einen Benutzernamen angeben.");
    }
    if req.sso_only && state.sso.is_none() {
        return error(400, "Auf diesem Server ist die Anmeldung mit Microsoft 365 nicht eingerichtet.");
    }
    let t = &mut state.tenants[ti];
    let before = t.stored.users.clone();
    let users = &mut t.stored.users;
    if let Some(u) = users.iter().find(|u| u.name.eq_ignore_ascii_case(&name)) {
        if u.role == Role::Admin && req.role != Role::Admin && users_admins(users) <= 1 {
            return error(409, "Es muss mindestens einen Administrator geben.");
        }
        let u = users.iter_mut().find(|u| u.name.eq_ignore_ascii_case(&name)).unwrap();
        u.role = req.role;
        if req.sso_only {
            *u = sso_user(&u.name.clone(), req.role);
        } else if !req.password.is_empty() {
            if req.password.len() < 6 {
                return error(400, "Das Passwort braucht mindestens 6 Zeichen.");
            }
            let totp = std::mem::take(&mut u.totp);
            *u = make_user(&u.name.clone(), req.role, &req.password);
            u.totp = totp;
        }
    } else if req.sso_only {
        users.push(sso_user(&name, req.role));
    } else {
        if req.password.len() < 6 {
            return error(400, "Das Passwort braucht mindestens 6 Zeichen.");
        }
        users.push(make_user(&name, req.role, &req.password));
    }
    // Sessions keep the role they logged in with; make changes take effect now.
    for s in state.sessions.values_mut().filter(|s| s.tenant == ti && s.user.eq_ignore_ascii_case(&name)) {
        s.role = req.role;
    }
    state.tenants[ti].save_users(before)
}

/// A further company on the server; the administrator who creates it becomes its first administrator.
fn create_tenant(state: &mut State, ti: usize, user: &str, name: &str) -> Reply {
    if name.is_empty() {
        return error(400, "Bitte einen Firmennamen angeben.");
    }
    let mut id: String = name
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'ä' => 'a',
            'ö' => 'o',
            'ü' => 'u',
            'ß' => 's',
            c if c.is_ascii_alphanumeric() => c,
            _ => '_',
        })
        .collect::<String>()
        .trim_matches('_')
        .chars()
        .take(30)
        .collect();
    if id.is_empty() {
        id = "firma".into();
    }
    let base = id.clone();
    let mut k = 2;
    while state.tenants.iter().any(|t| t.id == id) {
        id = format!("{base}_{k}");
        k += 1;
    }
    let Some(me) = state.tenants[ti].stored.users.iter().find(|u| u.name == user).cloned() else {
        return error(404, "Benutzer nicht gefunden.");
    };
    let first = User { role: Role::Admin, ..me };
    match open_tenant(&state.dir, state.database.as_deref(), &id, first) {
        Ok((mut t, _)) => {
            let mut copy = t.stored.data.clone();
            let mut company = copy.company.clone();
            company.name = name.to_string();
            let _ = copy.apply(Op::SaveCompany(company), user);
            if let Err(e) = t.commit(copy) {
                return error(500, &e);
            }
            start_inbox_checker(&mut t);
            state.tenants.push(t);
            json_response(200, &tenant_list(state))
        }
        Err(e) => error(500, &e),
    }
}
