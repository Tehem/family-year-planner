//! Kiosk sessions (§9) on the server side.

mod common;

use annee_server::tokens::Scope;
use axum::http::StatusCode;
use common::TestApp;
use serde_json::json;

async fn kiosk_app() -> (TestApp, String, String) {
    let app = TestApp::demo().await;
    let (pat, clear) = app
        .state
        .tokens
        .create("Kiosque – test", Scope::Read, true, "Alex")
        .await
        .unwrap();
    (app, pat.id, clear)
}

#[tokio::test]
async fn token_exchange_sets_a_cookie_and_removes_the_token_from_the_url() {
    let (mut app, _id, clear) = kiosk_app().await;
    let r = app.get(&format!("/kiosk?token={clear}")).await;
    assert_eq!(r.status, StatusCode::SEE_OTHER);
    assert_eq!(r.header("location"), Some("/kiosk"));
    let cookie = r
        .headers
        .get_all("set-cookie")
        .iter()
        .map(|v| v.to_str().unwrap().to_string())
        .find(|c| c.starts_with("annee_kiosk="))
        .unwrap();
    for needle in ["HttpOnly", "SameSite=Lax", "Path=/", "Max-Age=34560000"] {
        assert!(cookie.contains(needle), "{needle} in {cookie}");
    }
    assert!(
        !cookie.contains(&clear),
        "the cookie never contains the token"
    );
    assert_eq!(app.get("/kiosk").await.status, StatusCode::OK);
}

#[tokio::test]
async fn kiosk_sessions_are_read_only() {
    let (mut app, _id, clear) = kiosk_app().await;
    app.get(&format!("/kiosk?token={clear}")).await;
    let me = app.get("/api/me").await.json();
    assert_eq!(me["kiosk"], true);
    assert_eq!(app.get("/api/config").await.status, StatusCode::OK);
    assert_eq!(
        app.get("/api/events?year=2026").await.status,
        StatusCode::OK
    );
    // No writes, no admin, no main screen or settings.
    let body = json!({"type": "trip", "title": "x", "start": "2026-08-10", "end": "2026-08-12"});
    let r = app.post("/api/events", body).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    assert_eq!(
        r.json()["error"]["message"],
        "Cette session est en lecture seule."
    );
    assert_eq!(
        app.patch("/api/events/f:abc", json!({"title": "x"}))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        app.delete("/api/events/f:abc").await.status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        app.get("/api/admin/tokens").await.status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        app.get("/api/admin/import").await.status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(app.get("/").await.status, StatusCode::SEE_OTHER);
    assert_eq!(app.get("/reglages").await.status, StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn revoking_the_token_ends_the_kiosk_session() {
    let (mut app, id, clear) = kiosk_app().await;
    app.get(&format!("/kiosk?token={clear}")).await;
    assert_eq!(
        app.get("/api/events?year=2026").await.status,
        StatusCode::OK
    );
    app.state.tokens.revoke(&id).await.unwrap();
    assert_eq!(
        app.get("/api/events?year=2026").await.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(app.get("/kiosk").await.status, StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn bad_or_non_kiosk_tokens_are_refused() {
    let (mut app, _id, _clear) = kiosk_app().await;
    let (_, agent) = app
        .state
        .tokens
        .create("Agent", Scope::Read, false, "T")
        .await
        .unwrap();
    for t in ["fy_pat_nope", "garbage", agent.as_str()] {
        let r = app.get(&format!("/kiosk?token={t}")).await;
        assert_eq!(r.header("location"), Some("/connexion?error=kiosk"), "{t}");
    }
    assert_eq!(app.get("/api/me").await.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_web_session_can_open_the_kiosk_page() {
    let mut app = TestApp::demo().await;
    assert_eq!(
        app.get("/kiosk").await.header("location"),
        Some("/connexion?next=%2Fkiosk")
    );
    app.login().await;
    assert_eq!(app.get("/kiosk").await.status, StatusCode::OK);
}
