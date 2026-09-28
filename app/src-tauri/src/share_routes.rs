use actix_web::{get, post, web, HttpRequest, HttpResponse, Responder, cookie::Cookie};
use crate::commands::TelegramState;
use crate::commands::utils::resolve_peer;
use crate::db::DbConnection;
use sha2::{Sha256, Digest};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use serde::Deserialize;
use rand::RngExt;

#[derive(Clone)]
struct SharedLinkRow {
    _id: String,
    folder_id: Option<i64>,
    message_id: i32,
    file_name: String,
    _file_size: i64,
    password_hash: Option<String>,
    _password_salt: Option<String>,
    expires_at: Option<i64>,
    revoked: bool,
}

#[derive(Deserialize)]
struct VerifyForm {
    password: String,
}

/// Verify a password against a bcrypt hash.
fn verify_password(password: &str, hash: &str) -> bool {
    bcrypt::verify(password, hash).unwrap_or(false)
}

/// Brute-force backstop for the password-verify endpoint (R4 #5). bcrypt
/// already makes each guess expensive, but guesses were unlimited — so cap
/// FAILED attempts per share token: 5 wrong passwords per 10 minutes, then
/// the endpoint refuses to check until the window slides past. Successful
/// logins clear the counter. Per-process memory (like the cookie secret
/// below): a restart resets counters, which is acceptable for a
/// loopback-only server and avoids a new DB table.
const MAX_VERIFY_ATTEMPTS: usize = 5;
const VERIFY_WINDOW: Duration = Duration::from_secs(600);

static VERIFY_ATTEMPTS: OnceLock<Mutex<HashMap<String, VecDeque<Instant>>>> = OnceLock::new();

fn verify_attempts() -> &'static Mutex<HashMap<String, VecDeque<Instant>>> {
    VERIFY_ATTEMPTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn evict_stale(queue: &mut VecDeque<Instant>, now: Instant) {
    while queue
        .front()
        .is_some_and(|seen| now.duration_since(*seen) >= VERIFY_WINDOW)
    {
        queue.pop_front();
    }
}

/// Refuse to check when the token is over the failed-attempt budget.
/// Returns `Err(wait)` with how long the client should back off.
fn check_verify_lockout(token: &str) -> Result<(), Duration> {
    let mut attempts = verify_attempts()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let now = Instant::now();
    let mut queue = attempts.remove(token).unwrap_or_default();
    evict_stale(&mut queue, now);
    let locked = queue.len() >= MAX_VERIFY_ATTEMPTS;
    let wait = queue
        .front()
        .map(|seen| VERIFY_WINDOW.saturating_sub(now.duration_since(*seen)))
        .unwrap_or(VERIFY_WINDOW)
        .max(Duration::from_secs(1));
    if !queue.is_empty() {
        attempts.insert(token.to_string(), queue);
    }
    if locked {
        Err(wait)
    } else {
        Ok(())
    }
}

fn record_failed_verify(token: &str) {
    let mut attempts = verify_attempts()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let now = Instant::now();
    let mut queue = attempts.remove(token).unwrap_or_default();
    evict_stale(&mut queue, now);
    queue.push_back(now);
    attempts.insert(token.to_string(), queue);
}

fn clear_verify_attempts(token: &str) {
    if let Ok(mut attempts) = verify_attempts().lock() {
        attempts.remove(token);
    }
}

/// Per-process secret mixed into share-auth cookies. Reading `shares.db`
/// alone is not enough to mint a valid cookie; the secret dies with the app.
static SHARE_COOKIE_SECRET: OnceLock<[u8; 32]> = OnceLock::new();

fn share_cookie_secret() -> &'static [u8; 32] {
    SHARE_COOKIE_SECRET.get_or_init(|| {
        let mut rng = rand::rng();
        let mut bytes = [0u8; 32];
        for b in &mut bytes {
            *b = rng.random();
        }
        bytes
    })
}

fn generate_cookie_val(token: &str, password_hash: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(share_cookie_secret());
    hasher.update(token.as_bytes());
    hasher.update(password_hash.as_bytes());
    let digest = hasher.finalize();
    digest.iter().map(|b| format!("{:02x}", b)).collect()
}

fn get_share_by_token(db: &DbConnection, token: &str) -> Result<Option<SharedLinkRow>, String> {
    let conn = db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, folder_id, message_id, file_name, file_size, password_hash, password_salt, expires_at, revoked
             FROM shared_links WHERE id = ?"
        )
        .map_err(|e| e.to_string())?;

    stmt.bind((1, token)).map_err(|e| e.to_string())?;

    if let sqlite::State::Row = stmt.next().map_err(|e| e.to_string())? {
        let id = stmt.read::<String, _>("id").map_err(|e| e.to_string())?;
        let folder_id = stmt.read::<Option<i64>, _>("folder_id").ok().flatten();
        let message_id = stmt.read::<i64, _>("message_id").map_err(|e| e.to_string())? as i32;
        let file_name = stmt.read::<String, _>("file_name").map_err(|e| e.to_string())?;
        let file_size = stmt.read::<i64, _>("file_size").map_err(|e| e.to_string())?;
        let password_hash = stmt.read::<Option<String>, _>("password_hash").ok().flatten();
        let _password_salt = stmt.read::<Option<String>, _>("password_salt").ok().flatten();
        let expires_at = stmt.read::<Option<i64>, _>("expires_at").ok().flatten();
        let revoked = stmt.read::<i64, _>("revoked").map_err(|e| e.to_string())? != 0;

        Ok(Some(SharedLinkRow {
            _id: id,
            folder_id,
            message_id,
            file_name,
            _file_size: file_size,
            password_hash,
            _password_salt,
            expires_at,
            revoked,
        }))
    } else {
        Ok(None)
    }
}

/// Escape text for safe interpolation into HTML text nodes and attributes.
fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Renders the password entry form for protected share links.
///
/// Filename and token are HTML-escaped: file names are user/Telegram-controlled
/// and must never be interpreted as markup on this auth boundary.
///
/// NOTE: This HTML contains an inline `<style>` block which requires
/// `style-src 'unsafe-inline'` in the Tauri CSP (tauri.conf.json).
/// This is acceptable because the page is served only over the local
/// Actix streaming server on loopback, not the public internet.
fn render_password_form(file_name: &str, token: &str, error: Option<&str>) -> HttpResponse {
    let error_html = match error {
        Some(err) => format!("<div class=\"error\">{}</div>", escape_html(err)),
        None => "".to_string(),
    };

    let html = format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>Password Protected File - TeleStash</title>
    <style>
        body {{
            background-color: #182533;
            color: #ffffff;
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
            display: flex;
            align-items: center;
            justify-content: center;
            height: 100vh;
            margin: 0;
        }}
        .container {{
            background: #202b36;
            padding: 2rem;
            border-radius: 12px;
            box-shadow: 0 8px 24px rgba(0, 0, 0, 0.2);
            border: 1px solid #2f3e4e;
            width: 100%;
            max-width: 400px;
            text-align: center;
        }}
        h2 {{
            margin-top: 0;
            color: #40a7e3;
        }}
        p {{
            font-size: 14px;
            color: #7f91a4;
            margin-bottom: 20px;
        }}
        input[type="password"] {{
            width: 100%;
            padding: 12px;
            border-radius: 6px;
            border: 1px solid #2f3e4e;
            background: #182533;
            color: white;
            box-sizing: border-box;
            margin-bottom: 15px;
            font-size: 16px;
        }}
        input[type="password"]:focus {{
            outline: none;
            border-color: #40a7e3;
        }}
        button {{
            width: 100%;
            padding: 12px;
            border-radius: 6px;
            border: none;
            background: #40a7e3;
            color: white;
            font-weight: bold;
            cursor: pointer;
            font-size: 16px;
            transition: background 0.2s;
        }}
        button:hover {{
            background: #3598d1;
        }}
        .error {{
            color: #ff5e5e;
            font-size: 14px;
            margin-bottom: 15px;
        }}
    </style>
</head>
<body>
    <div class="container">
        <h2>Enter Password</h2>
        <p>This share link is password-protected.<br>File: <strong>{}</strong></p>
        {}
        <form method="POST" action="/d/{}/verify">
            <input type="password" name="password" placeholder="Password" autofocus required>
            <button type="submit">Verify & Download</button>
        </form>
    </div>
</body>
</html>"#,
        escape_html(file_name),
        error_html,
        escape_html(token)
    );

    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(html)
}

#[get("/d/{token}")]
async fn get_shared_file(
    req: HttpRequest,
    path: web::Path<String>,
    db_conn: web::Data<DbConnection>,
    tg_state: web::Data<Arc<TelegramState>>,
) -> impl Responder {
    let token = path.into_inner();

    let row = match get_share_by_token(&db_conn, &token) {
        Ok(Some(r)) => r,
        Ok(None) => return HttpResponse::NotFound().body("Shared link not found"),
        Err(e) => {
            log::error!("DB error resolving token {}: {}", token, e);
            return HttpResponse::InternalServerError().body("Internal server error")
        }
    };

    // Check validation (revocation and expiration)
    if row.revoked {
        return HttpResponse::NotFound().body("This shared link has been revoked");
    }

    if let Some(expiry) = row.expires_at {
        let now = chrono::Utc::now().timestamp();
        if expiry < now {
            return HttpResponse::Gone().body("This shared link has expired");
        }
    }

    // Check password protection
    if let Some(hash) = &row.password_hash {
        let mut authenticated = false;
        if let Some(cookie) = req.cookie(&format!("share_auth_{}", token)) {
            let expected = generate_cookie_val(&token, hash);
            if cookie.value() == expected {
                authenticated = true;
            }
        }

        if !authenticated {
            return render_password_form(&row.file_name, &token, None);
        }
    }

    // Retrieve and stream the file from Telegram
    let client_opt = { tg_state.client.lock().await.clone() };
    let client = match client_opt {
        Some(c) => c,
        None => return HttpResponse::ServiceUnavailable().body("Telegram client is not connected"),
    };

    let peer = match resolve_peer(&client, row.folder_id, &tg_state.peer_cache).await {
        Ok(p) => p,
        Err(e) => {
            log::error!("Failed to resolve peer for share: {}", e);
            return HttpResponse::InternalServerError().body("Failed to locate folder");
        }
    };

    match client.get_messages_by_id(peer, &[row.message_id]).await {
        Ok(messages) => {
            if let Some(Some(msg)) = messages.first() {
                if let Some(media) = msg.media() {
                    let mime = crate::commands::utils::mime_type_from_media(&media);
                    let filename = &row.file_name;

                    return crate::server::build_media_response(
                        &client, &media, &req, &mime, Some(filename),
                        crate::server::StreamingExtras {
                            extra_headers: vec![],
                            log_label: "Share download",
                        },
                        Some((peer, row.message_id)),
                    );
                }
            }
            HttpResponse::NotFound().body("Message or media not found in Telegram")
        }
        Err(e) => {
            log::error!("Failed to fetch shared message {}: {}", row.message_id, e);
            HttpResponse::InternalServerError().body(format!("Failed to retrieve file: {}", e))
        }
    }
}

#[post("/d/{token}/verify")]
async fn verify_shared_file_password(
    path: web::Path<String>,
    form: web::Form<VerifyForm>,
    db_conn: web::Data<DbConnection>,
) -> impl Responder {
    let token = path.into_inner();

    let row = match get_share_by_token(&db_conn, &token) {
        Ok(Some(r)) => r,
        Ok(None) => return HttpResponse::NotFound().body("Shared link not found"),
        Err(e) => {
            log::error!("DB error resolving token {}: {}", token, e);
            return HttpResponse::InternalServerError().body("Internal server error")
        }
    };

    if row.revoked {
        return HttpResponse::NotFound().body("This shared link has been revoked");
    }

    let hash = match &row.password_hash {
        Some(h) => h,
        None => return HttpResponse::BadRequest().body("No password required for this link"),
    };

    // R4 #5: refuse to even check the password once this token has burned
    // through its failed-attempt budget.
    if let Err(wait) = check_verify_lockout(&token) {
        let minutes = wait.as_secs().div_ceil(60);
        return render_password_form(
            &row.file_name,
            &token,
            Some(&format!(
                "Too many wrong passwords. Try again in about {} minute{}.",
                minutes,
                if minutes == 1 { "" } else { "s" }
            )),
        );
    }

    if verify_password(&form.password, hash) {
        clear_verify_attempts(&token);
        // Set session cookie (30 min).
        // NOTE: The share server binds to 127.0.0.1 over plain HTTP (not HTTPS),
        // so the cookie cannot use `.secure(true)` without becoming unusable.
        // The cookie is protected by `.http_only(true)` and `.same_site(Strict)`
        // and mixed with a per-process secret so a DB dump alone cannot mint it.
        let val = generate_cookie_val(&token, hash);
        let cookie = Cookie::build(format!("share_auth_{}", token), val)
            .path(format!("/d/{}", token))
            .http_only(true)
            .same_site(actix_web::cookie::SameSite::Strict)
            .max_age(actix_web::cookie::time::Duration::minutes(30))
            .finish();

        HttpResponse::Found()
            .insert_header(("Location", format!("/d/{}", token)))
            .cookie(cookie)
            .finish()
    } else {
        record_failed_verify(&token);
        render_password_form(&row.file_name, &token, Some("Incorrect password. Please try again."))
    }
}

pub fn configure_share_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(get_shared_file)
       .service(verify_shared_file_password);
}

#[cfg(test)]
mod tests {
    use super::escape_html;
    use super::{check_verify_lockout, clear_verify_attempts, record_failed_verify};

    #[test]
    fn escapes_html_metacharacters_in_filenames() {
        assert_eq!(
            escape_html(r#"<img src=x onerror=alert(1)>.mkv"#),
            "&lt;img src=x onerror=alert(1)&gt;.mkv"
        );
        assert_eq!(escape_html("a&b"), "a&amp;b");
        assert_eq!(escape_html(r#"say "hi" & 'bye'"#), "say &quot;hi&quot; &amp; &#39;bye&#39;");
    }

    #[test]
    fn leaves_ordinary_filenames_unchanged() {
        assert_eq!(
            escape_html("Movie.Title.2024.1080p.mkv"),
            "Movie.Title.2024.1080p.mkv"
        );
    }

    #[test]
    fn five_wrong_passwords_lock_the_token_out() {
        let token = "r4-lockout-budget";
        clear_verify_attempts(token);
        for _ in 0..5 {
            assert!(check_verify_lockout(token).is_ok());
            record_failed_verify(token);
        }
        let wait = check_verify_lockout(token).expect_err("6th check must lock out");
        assert!(wait >= std::time::Duration::from_secs(1));
        clear_verify_attempts(token);
    }

    #[test]
    fn successful_login_clears_the_failed_counter() {
        let token = "r4-lockout-cleared";
        clear_verify_attempts(token);
        for _ in 0..5 {
            record_failed_verify(token);
        }
        assert!(check_verify_lockout(token).is_err());
        clear_verify_attempts(token);
        assert!(check_verify_lockout(token).is_ok());
        clear_verify_attempts(token);
    }

    #[test]
    fn lockout_is_per_token() {
        let locked = "r4-lockout-per-token-a";
        let fresh = "r4-lockout-per-token-b";
        clear_verify_attempts(locked);
        clear_verify_attempts(fresh);
        for _ in 0..5 {
            record_failed_verify(locked);
        }
        assert!(check_verify_lockout(locked).is_err());
        assert!(check_verify_lockout(fresh).is_ok());
        clear_verify_attempts(locked);
        clear_verify_attempts(fresh);
    }
}
