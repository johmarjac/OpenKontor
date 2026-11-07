//! The mobile web app the server delivers at its address (e.g. http://server:8470/).
//! It works in the browser of any phone or tablet and can be added to the home screen:
//! photograph receipts, see open invoices and record payments and expenses, look up contacts.

pub const MANIFEST: &str = r##"{
  "name": "OpenKontor",
  "short_name": "OpenKontor",
  "start_url": "/",
  "display": "standalone",
  "background_color": "#152e47",
  "theme_color": "#152e47",
  "icons": [{ "src": "/icon.svg", "sizes": "any", "type": "image/svg+xml", "purpose": "any maskable" }]
}"##;

pub const ICON: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512"><rect width="512" height="512" rx="96" fill="#152e47"/><rect x="120" y="104" width="272" height="320" rx="28" fill="#fff"/><rect x="160" y="168" width="192" height="22" rx="11" fill="#2fb39c"/><rect x="160" y="226" width="150" height="18" rx="9" fill="#9fb3c6"/><rect x="160" y="274" width="170" height="18" rx="9" fill="#9fb3c6"/><rect x="160" y="340" width="110" height="26" rx="13" fill="#1f6f8b"/></svg>"##;

pub const PAGE: &str = r##"<!doctype html>
<html lang="de">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">
<meta name="theme-color" content="#152e47">
<meta name="apple-mobile-web-app-capable" content="yes">
<link rel="manifest" href="/manifest.json">
<link rel="icon" href="/icon.svg">
<link rel="apple-touch-icon" href="/icon.svg">
<title>OpenKontor</title>
<style>
*{box-sizing:border-box}body{margin:0;font:16px/1.4 -apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,sans-serif;background:#f2f4f7;color:#1d2733}
header{background:#152e47;color:#fff;padding:14px 16px calc(14px) 16px;padding-top:max(14px,env(safe-area-inset-top));display:flex;align-items:center;gap:10px}
header b{font-size:18px}header span{margin-left:auto;font-size:13px;color:#c9d6e3}
main{padding:14px 14px 90px}nav{position:fixed;bottom:0;left:0;right:0;background:#fff;border-top:1px solid #e0e5eb;display:flex;padding-bottom:env(safe-area-inset-bottom)}
nav button{flex:1;border:0;background:none;padding:10px 4px;font-size:12px;color:#6b7785}nav button.on{color:#1f6f8b;font-weight:600}
nav svg{display:block;margin:0 auto 3px;width:24px;height:24px}
.card{background:#fff;border:1px solid #e0e5eb;border-radius:12px;padding:14px;margin-bottom:12px}
h2{font-size:17px;margin:0 0 10px}label{display:block;font-size:13px;color:#6b7785;margin:10px 0 4px}
input,select{width:100%;font-size:16px;padding:11px;border:1px solid #cfd6de;border-radius:8px;background:#fff}
button.p{width:100%;background:#1f6f8b;color:#fff;border:0;border-radius:10px;padding:14px;font-size:16px;font-weight:600;margin-top:14px}
button.s{background:#fff;color:#1f6f8b;border:1px solid #1f6f8b;border-radius:8px;padding:8px 12px;font-size:14px}
.row{display:flex;justify-content:space-between;gap:10px;align-items:center;padding:10px 0;border-bottom:1px solid #eef1f4}.row:last-child{border:0}
.muted{color:#6b7785;font-size:13px}.red{color:#c43d3d}.green{color:#1f8a55}.err{color:#c43d3d;margin-top:10px}.ok{color:#1f8a55;margin-top:10px}
.big{font-size:22px;font-weight:700}.chip{font-size:12px;padding:2px 8px;border-radius:9px;background:#ecefF3;color:#55606e}
.drop{display:block;text-align:center;border:2px dashed #9fb3c6;border-radius:12px;padding:28px 10px;color:#1f6f8b;font-weight:600}
.two{display:flex;gap:10px}.two>*{flex:1}
</style>
</head>
<body>
<header><b>OpenKontor</b><span id="who"></span></header>
<main id="main"></main>
<nav id="nav" hidden></nav>
<script>
"use strict";
const de = !navigator.language.toLowerCase().startsWith("en");
const T = (g, e) => de ? g : e;
let token = localStorage.getItem("ok_token"), me = JSON.parse(localStorage.getItem("ok_me") || "null"), data = null, tab = "beleg";
const $ = s => document.querySelector(s);
const esc = s => String(s ?? "").replace(/[&<>"]/g, c => ({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;"}[c]));
const money = c => (c / 100).toLocaleString(de ? "de-DE" : "en-GB", {style: "currency", currency: "EUR"});
const day = d => d ? new Date(d + "T00:00:00").toLocaleDateString(de ? "de-DE" : "en-GB") : "";
const today = () => { const d = new Date(); return d.getFullYear() + "-" + String(d.getMonth() + 1).padStart(2, "0") + "-" + String(d.getDate()).padStart(2, "0"); };
async function api(method, path, body, raw) {
  const h = token ? {Authorization: "Bearer " + token} : {};
  if (body !== undefined && !raw) h["Content-Type"] = "application/json";
  const r = await fetch(path, {method, headers: h, body: body === undefined ? undefined : raw ? body : JSON.stringify(body)});
  const j = await r.json().catch(() => ({}));
  if (r.status === 401 && path !== "/api/login") { logout(); throw new Error(j.error || "Bitte neu anmelden."); }
  if (!r.ok) throw Object.assign(new Error(j.error || r.statusText), j);
  return j;
}
function logout() { token = null; me = null; localStorage.removeItem("ok_token"); localStorage.removeItem("ok_me"); login(); }
const can = area => me && ({Admin: 1, Buchhaltung: 1}[me.role] || (me.role === "Vertrieb" && (area === "sales" || area === "receipts")) || (me.role === "Belegerfassung" && area === "receipts"));

async function login(msg) {
  $("#nav").hidden = true; $("#who").textContent = "";
  let info = {}; try { info = await api("GET", "/api/info"); } catch (e) {}
  const tenants = info.tenants || [];
  $("#main").innerHTML = `<div class="card"><h2>${T("Anmelden", "Sign in")}</h2><div class="muted">${esc(info.name || "")}</div>
    ${tenants.length > 1 ? `<label>${T("Firma", "Company")}</label><select id="ten">${tenants.map(t => `<option value="${esc(t.id)}">${esc(t.name)}</option>`).join("")}</select>` : ""}
    <label>${T("Benutzer", "User")}</label><input id="u" autocomplete="username" autocapitalize="off">
    <label>${T("Passwort", "Password")}</label><input id="pw" type="password" autocomplete="current-password">
    <div id="codebox" hidden><label>${T("Bestätigungscode (Authenticator-App)", "Verification code (authenticator app)")}</label><input id="code" inputmode="numeric" autocomplete="one-time-code"></div>
    <button class="p" id="go">${T("Anmelden", "Sign in")}</button><div class="err" id="e">${esc(msg || "")}</div></div>`;
  $("#go").onclick = async () => {
    try {
      const r = await api("POST", "/api/login", {user: $("#u").value, password: $("#pw").value, code: $("#code").value, tenant: $("#ten") ? $("#ten").value : ""});
      token = r.token; me = {user: r.user, role: r.role, server: r.server_name, small: false};
      localStorage.setItem("ok_token", token); localStorage.setItem("ok_me", JSON.stringify(me));
      start();
    } catch (e) {
      if (e.need_code) $("#codebox").hidden = false;
      $("#e").textContent = e.message;
    }
  };
}

const icons = {
  beleg: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M4 8h3l2-3h6l2 3h3v11H4z"/><circle cx="12" cy="13" r="4"/></svg>',
  offen: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M6 3h9l4 4v14H6z"/><path d="M9 12h7M9 16h5"/></svg>',
  ausgabe: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="6" width="18" height="13" rx="2"/><path d="M3 10h18M15 15h3"/></svg>',
  kontakte: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="8" r="4"/><path d="M4 21c1-4 4-6 8-6s7 2 8 6"/></svg>',
};
function navbar() {
  const tabs = [["beleg", T("Beleg", "Receipt"), "receipts"], ["offen", T("Offen", "Open"), "sales"], ["ausgabe", T("Ausgabe", "Expense"), "accounting"], ["kontakte", T("Kontakte", "Contacts"), "read"]]
    .filter(t => t[2] === "read" || can(t[2]));
  if (!tabs.some(t => t[0] === tab)) tab = tabs[0][0];
  $("#nav").innerHTML = tabs.map(t => `<button class="${t[0] === tab ? "on" : ""}" data-t="${t[0]}">${icons[t[0]]}${t[1]}</button>`).join("");
  $("#nav").hidden = false;
  $("#nav").querySelectorAll("button").forEach(b => b.onclick = () => { tab = b.dataset.t; render(); });
}
async function load() { const r = await api("GET", "/api/state?since=0"); data = r.data; }
async function start() {
  $("#who").textContent = me.user;
  $("#who").onclick = () => { if (confirm(T("Abmelden?", "Sign out?"))) logout(); };
  try { await load(); render(); } catch (e) { login(e.message); }
}
function totals(d) {
  const small = data.company.small_business, rates = {};
  for (const l of d.lines) { const net = Math.round(l.qty * l.price_net * (1 - (l.discount_pct || 0) / 100)); const r = small || (d.vat_case && d.vat_case !== "Inland") ? 0 : l.vat; rates[r] = (rates[r] || 0) + net; }
  let gross = 0; for (const r in rates) gross += rates[r] + Math.round(rates[r] * r / 100);
  return d.kind === "Gutschrift" ? -gross : gross;
}
const paid = d => (d.payments || []).reduce((s, p) => s + p.amount + (p.discount || 0), 0);
const dueDate = d => { const x = new Date(d.date + "T00:00:00"); x.setDate(x.getDate() + d.due_days); return x; };
const contactName = id => { const c = data.contacts.find(c => c.id === id); return c ? (c.company || c.person) : "–"; };
function render() { navbar(); ({beleg: viewReceipt, offen: viewOpen, ausgabe: viewExpense, kontakte: viewContacts})[tab](); window.scrollTo(0, 0); }

function viewReceipt() {
  const recent = data.receipts.slice(-8).reverse();
  $("#main").innerHTML = `<div class="card"><h2>${T("Beleg erfassen", "Add receipt")}</h2>
    <label class="drop" for="f">${T("Foto aufnehmen oder Datei wählen", "Take a photo or choose a file")}</label>
    <input id="f" type="file" accept="image/*,application/pdf,.xml" capture="environment" hidden>
    <div id="m" class="muted" style="margin-top:10px">${T("Der Server liest Lieferant, Datum und Betrag automatisch aus.", "The server reads supplier, date and amount automatically.")}</div></div>
    <div class="card"><h2>${T("Zuletzt eingegangen", "Recently received")}</h2>${recent.length ? recent.map(r => `<div class="row"><div><b>${esc(r.supplier || r.file_name)}</b><div class="muted">${esc(r.received)} · ${esc(r.number)}</div></div><div style="text-align:right">${r.gross ? money(r.gross) : ""}<div><span class="chip">${esc(r.status)}</span></div></div></div>`).join("") : `<div class="muted">${T("Noch keine Belege.", "No receipts yet.")}</div>`}</div>`;
  $("#f").onchange = async e => {
    const file = e.target.files[0]; if (!file) return;
    $("#m").textContent = T("Wird hochgeladen …", "Uploading …");
    try {
      let body = file, name = file.name || "foto.jpg";
      if (file.type.startsWith("image/")) { body = await shrink(file); name = name.replace(/\.[^.]+$/, "") + ".jpg"; }
      const r = await api("POST", "/api/receipts?name=" + encodeURIComponent(name), body, true);
      data = r.data; render(); $("#m").className = "ok"; $("#m").textContent = T("Beleg ist im Belegeingang.", "The receipt is in the inbox.");
    } catch (err) { $("#m").className = "err"; $("#m").textContent = err.message; }
  };
}
// Photos are made smaller (max. 2200 px) before sending; enough for the text recognition.
function shrink(file) {
  return new Promise(ok => {
    const img = new Image(); img.onload = () => {
      const k = Math.min(1, 2200 / Math.max(img.width, img.height)), c = document.createElement("canvas");
      c.width = Math.round(img.width * k); c.height = Math.round(img.height * k);
      c.getContext("2d").drawImage(img, 0, 0, c.width, c.height); c.toBlob(b => ok(b || file), "image/jpeg", 0.85);
    }; img.onerror = () => ok(file); img.src = URL.createObjectURL(file);
  });
}
function viewOpen() {
  const now = new Date(); now.setHours(0, 0, 0, 0);
  const list = data.documents.filter(d => (d.kind === "Rechnung" || d.kind === "Gutschrift") && d.status === "Offen").sort((a, b) => dueDate(a) - dueDate(b));
  const sum = list.reduce((s, d) => s + totals(d) - paid(d), 0);
  $("#main").innerHTML = `<div class="card"><div class="muted">${T("Offene Posten", "Open items")}</div><div class="big">${money(sum)}</div><div class="muted">${list.length} ${T("Belege", "documents")}</div></div>
    <div class="card">${list.length ? list.map(d => { const late = dueDate(d) < now; return `<div class="row"><div><b>${esc(contactName(d.contact_id))}</b><div class="muted ${late ? "red" : ""}">${esc(d.number)} · ${T("fällig", "due")} ${dueDate(d).toLocaleDateString(de ? "de-DE" : "en-GB")}</div></div>
      <div style="text-align:right"><b>${money(totals(d) - paid(d))}</b><div><button class="s" data-pay="${d.id}">${T("Bezahlt", "Paid")}</button></div></div></div>`; }).join("") : `<div class="muted">${T("Alles bezahlt.", "Everything is paid.")}</div>`}</div><div id="m"></div>`;
  document.querySelectorAll("[data-pay]").forEach(b => b.onclick = async () => {
    const d = data.documents.find(x => x.id == b.dataset.pay);
    if (!confirm(T(`${d.number} heute als bezahlt buchen?`, `Book ${d.number} as paid today?`))) return;
    try { const r = await api("POST", "/api/op", {MarkPaid: [d.id, today()]}); data = r.data; render(); }
    catch (e) { $("#m").className = "err"; $("#m").textContent = e.message; }
  });
}
function viewExpense() {
  const cats = ["3400 Wareneingang","4210 Miete","4240 Strom, Gas, Wasser","4360 Versicherungen","4530 Kfz-Kosten","4600 Werbekosten","4650 Bewirtung","4670 Reisekosten","4930 Bürobedarf","4920 Telefon und Internet","4940 Zeitschriften, Bücher","4950 Rechts- und Beratungskosten","4970 Kosten des Geldverkehrs","4855 Geringwertige Wirtschaftsgüter","1800 Privatentnahme"];
  $("#main").innerHTML = `<div class="card"><h2>${T("Ausgabe erfassen", "Record expense")}</h2>
    <div class="two"><div><label>${T("Datum", "Date")}</label><input id="d" type="date" value="${today()}"></div><div><label>${T("Betrag brutto", "Gross amount")}</label><input id="a" inputmode="decimal" placeholder="0,00"></div></div>
    <label>${T("Wofür?", "What for?")}</label><input id="t" placeholder="${T("z. B. Tankquittung", "e.g. fuel receipt")}">
    <label>${T("Kategorie", "Category")}</label><select id="c">${cats.map(c => `<option ${c.startsWith("4930") ? "selected" : ""}>${c}</option>`).join("")}</select>
    <div class="two"><div><label>${T("Steuersatz", "VAT rate")}</label><select id="v"><option>19</option><option>7</option><option>0</option></select></div>
    <div><label>${T("Bezahlt mit", "Paid by")}</label><select id="w"><option value="Kasse">${T("Bar (Kasse)", "Cash")}</option><option value="Bank">Bank</option><option value="Karte">${T("Karte", "Card")}</option></select></div></div>
    <button class="p" id="go">${T("Speichern", "Save")}</button><div id="m"></div></div>`;
  $("#go").onclick = async () => {
    const amount = Math.round(parseFloat($("#a").value.replace(/\./g, "").replace(",", ".")) * 100);
    if (!(amount > 0)) { $("#m").className = "err"; $("#m").textContent = T("Bitte einen Betrag eingeben.", "Please enter an amount."); return; }
    try {
      const r = await api("POST", "/api/op", {SaveBooking: {date: $("#d").value, kind: "Ausgabe", category: $("#c").value, text: $("#t").value, gross: amount, vat: +$("#v").value, payment: $("#w").value}});
      data = r.data; viewExpense(); $("#m").className = "ok"; $("#m").textContent = T("Gespeichert.", "Saved.");
    } catch (e) { $("#m").className = "err"; $("#m").textContent = e.message; }
  };
}
function viewContacts() {
  $("#main").innerHTML = `<div class="card"><input id="q" placeholder="${T("Name, Ort, Nummer …", "Name, city, number …")}"></div><div class="card" id="l"></div>`;
  const show = () => {
    const q = $("#q").value.toLowerCase();
    const list = data.contacts.filter(c => !q || [c.company, c.person, c.zip_city, c.number].join(" ").toLowerCase().includes(q)).slice(0, 60);
    $("#l").innerHTML = list.map(c => `<div class="row"><div><b>${esc(c.company || c.person)}</b><div class="muted">${esc(c.person && c.company ? c.person + " · " : "")}${esc(c.zip_city)}</div></div>
      <div style="text-align:right">${c.phone ? `<a href="tel:${esc(c.phone.replace(/\s/g, ""))}">${esc(c.phone)}</a><br>` : ""}${c.email ? `<a href="mailto:${esc(c.email)}">${T("E-Mail", "E-mail")}</a>` : ""}</div></div>`).join("") || `<div class="muted">${T("Keine Kontakte gefunden.", "No contacts found.")}</div>`;
  };
  $("#q").oninput = show; show();
}
if (token && me) start(); else login();
</script>
</body>
</html>
"##;
