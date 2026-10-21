//! MCP server (§13) and personal access tokens (§11.3), driven through the official `rmcp` client.

mod common;

use annee_server::state::AppState;
use annee_server::tokens::Scope;
use axum::http::StatusCode;
use common::{TestApp, demo_state, spawn_server};
use rmcp::model::{CallToolRequestParams, CallToolResult};
use rmcp::service::RunningService;
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::{RoleClient, ServiceExt};
use serde_json::{Value, json};

type Client = RunningService<RoleClient, ()>;

async fn connect(base: &str, token: &str) -> Client {
    let transport = StreamableHttpClientTransport::from_config(
        StreamableHttpClientTransportConfig::with_uri(format!("{base}/mcp"))
            .auth_header(token.to_string()),
    );
    ().serve(transport).await.expect("MCP client connects")
}

async fn call(client: &Client, tool: &str, args: Value) -> CallToolResult {
    let params = CallToolRequestParams::new(tool.to_string())
        .with_arguments(args.as_object().cloned().unwrap_or_default());
    client
        .call_tool(params)
        .await
        .unwrap_or_else(|e| panic!("{tool} failed: {e}"))
}

fn structured(r: &CallToolResult) -> Value {
    r.structured_content
        .clone()
        .unwrap_or_else(|| panic!("no structured content: {r:?}"))
}

fn text(r: &CallToolResult) -> String {
    r.content
        .iter()
        .filter_map(|c| c.as_text().map(|t| t.text.clone()))
        .collect::<Vec<_>>()
        .join("")
}

async fn setup() -> (AppState, String) {
    let state = demo_state().await;
    let base = spawn_server(state.clone()).await;
    (state, base)
}

async fn pat(state: &AppState, name: &str, scope: Scope) -> String {
    state
        .tokens
        .create(name, scope, false, "Alex")
        .await
        .unwrap()
        .1
}

#[tokio::test]
async fn unauthenticated_requests_get_401_with_resource_metadata() {
    let (state, base) = setup().await;
    let http = reqwest::Client::new();
    let challenge =
        "Bearer resource_metadata=\"http://localhost:8080/.well-known/oauth-protected-resource\"";
    let post = |auth: Option<&str>| {
        let mut r = http
            .post(format!("{base}/mcp"))
            .header("content-type", "application/json")
            .header("accept", "application/json, text/event-stream")
            .body(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#);
        if let Some(a) = auth {
            r = r.header("authorization", a);
        }
        r.send()
    };
    for auth in [
        None,
        Some("Bearer fy_pat_unknown"),
        Some("fy_pat_unknown"),
        Some("Bearer garbage"),
        Some("Basic abc"),
    ] {
        let r = post(auth).await.unwrap();
        assert_eq!(r.status(), 401, "{auth:?}");
        assert_eq!(
            r.headers().get("www-authenticate").unwrap(),
            challenge,
            "{auth:?}"
        );
    }
    // A web session cookie is never accepted on /mcp.
    let mut app = TestApp::with_state(state.clone());
    app.login().await;
    let cookie = app
        .cookies()
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("; ");
    let r = http
        .post(format!("{base}/mcp"))
        .header("cookie", cookie)
        .header("content-type", "application/json")
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 401);
}

#[tokio::test]
async fn revoked_and_kiosk_tokens_are_refused() {
    let (state, base) = setup().await;
    let http = reqwest::Client::new();
    let (p, clear) = state
        .tokens
        .create("Temp", Scope::Read, false, "T")
        .await
        .unwrap();
    let ping = |tok: String| {
        http.post(format!("{base}/mcp"))
            .header("authorization", format!("Bearer {tok}"))
            .header("content-type", "application/json")
            .header("accept", "application/json, text/event-stream")
            .body(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#)
            .send()
    };
    assert_ne!(ping(clear.clone()).await.unwrap().status(), 401);
    state.tokens.revoke(&p.id).await.unwrap();
    assert_eq!(ping(clear).await.unwrap().status(), 401);
    let (_, kiosk) = state
        .tokens
        .create("Kiosque", Scope::Read, true, "T")
        .await
        .unwrap();
    assert_eq!(
        ping(kiosk).await.unwrap().status(),
        401,
        "kiosk links are not agent credentials"
    );
}

#[tokio::test]
async fn foreign_origins_are_rejected() {
    let (state, base) = setup().await;
    let token = pat(&state, "Browser", Scope::Read).await;
    let r = reqwest::Client::new()
        .post(format!("{base}/mcp"))
        .header("authorization", format!("Bearer {token}"))
        .header("origin", "https://evil.example")
        .header("content-type", "application/json")
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 403);
}

#[tokio::test]
async fn bare_token_without_scheme_is_accepted() {
    let (state, base) = setup().await;
    let token = pat(&state, "Grok API", Scope::Read).await;
    // rmcp's client always sends "Bearer"; use a raw request for the bare form.
    let r = reqwest::Client::new()
        .post(format!("{base}/mcp"))
        .header("authorization", &token)
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .body(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#)
        .send()
        .await
        .unwrap();
    assert_ne!(r.status(), 401);
}

#[tokio::test]
async fn server_identity_instructions_and_tool_catalogue() {
    let (state, base) = setup().await;
    let client = connect(&base, &pat(&state, "Claude", Scope::ReadWrite).await).await;
    let info = client.peer_info().expect("server info");
    let server = info.server_info.clone().expect("server implementation");
    assert_eq!(server.name, "family-year-planner");
    assert_eq!(server.version, env!("CARGO_PKG_VERSION"));
    let instr = info.instructions.clone().unwrap();
    for needle in [
        "Repères",
        "Famille",
        "INCLUSIVE",
        "Europe/Paris",
        "get_context",
        "family_home",
        "French",
    ] {
        assert!(instr.contains(needle), "instructions mention {needle}");
    }
    let tools = client.list_all_tools().await.unwrap();
    let names: Vec<_> = tools.iter().map(|t| t.name.to_string()).collect();
    assert_eq!(
        names,
        [
            "get_context",
            "list_events",
            "get_day",
            "find_free_periods",
            "create_event",
            "update_event",
            "delete_event"
        ]
    );
    let by = |n: &str| tools.iter().find(|t| t.name == n).unwrap();
    for ro in ["get_context", "list_events", "get_day", "find_free_periods"] {
        assert_eq!(
            by(ro).annotations.as_ref().unwrap().read_only_hint,
            Some(true),
            "{ro}"
        );
    }
    assert_eq!(
        by("delete_event")
            .annotations
            .as_ref()
            .unwrap()
            .destructive_hint,
        Some(true)
    );
    assert_eq!(
        by("update_event")
            .annotations
            .as_ref()
            .unwrap()
            .idempotent_hint,
        Some(true)
    );
    assert!(
        by("find_free_periods")
            .description
            .as_ref()
            .unwrap()
            .contains("Birthdays, daycare closures and public holidays do not make a day busy")
    );
    for t in &tools {
        assert!(
            t.description.as_ref().is_some_and(|d| d.len() > 40),
            "{} has a real description",
            t.name
        );
        assert_eq!(t.input_schema.get("type"), Some(&json!("object")));
    }
    client.cancel().await.ok();
}

#[tokio::test]
async fn read_tools_work_with_a_read_only_token() {
    let (state, base) = setup().await;
    let client = connect(&base, &pat(&state, "Lecteur", Scope::Read).await).await;

    let ctx = structured(&call(&client, "get_context", json!({})).await);
    assert_eq!(ctx["today"], "2026-10-16");
    assert_eq!(ctx["timezone"], "Europe/Paris");
    assert_eq!(ctx["members"], json!(["Alex", "Camille", "Sacha", "Noé"]));
    assert_eq!(ctx["school_zone"], "Zone B");
    let trip = ctx["types"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["code"] == "trip")
        .unwrap();
    assert_eq!(trip["label"], "Voyage ou vacances");
    assert_eq!(trip["calendar"], "famille");

    // Text content is the same JSON as the structured content.
    let r = call(&client, "get_context", json!({})).await;
    assert_eq!(
        serde_json::from_str::<Value>(&text(&r)).unwrap(),
        structured(&r)
    );

    let r = structured(
        &call(
            &client,
            "list_events",
            json!({"from": "2026-07-01", "to": "2026-08-31"}),
        )
        .await,
    );
    let titles: Vec<_> = r["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["title"].as_str().unwrap().to_string())
        .collect();
    assert!(titles.contains(&"Road trip en Italie".to_string()));
    assert!(titles.contains(&"Fête nationale".to_string()));
    let starts: Vec<_> = r["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["start"].as_str().unwrap().to_string())
        .collect();
    assert!(starts.windows(2).all(|w| w[0] <= w[1]), "sorted by start");

    let r = structured(
        &call(
            &client,
            "list_events",
            json!({"from": "2026-01-01", "to": "2026-12-31", "types": ["trip"]}),
        )
        .await,
    );
    assert_eq!(r["events"].as_array().unwrap().len(), 3);
    let r = structured(
        &call(
            &client,
            "list_events",
            json!({"from": "2026-01-01", "to": "2026-12-31", "query": "LISBONNE"}),
        )
        .await,
    );
    assert_eq!(r["events"].as_array().unwrap().len(), 1);
    let r = structured(
        &call(
            &client,
            "list_events",
            json!({"from": "2026-01-01", "to": "2026-12-31", "types": ["trip"], "member": "Sacha"}),
        )
        .await,
    );
    assert_eq!(
        r["events"].as_array().unwrap().len(),
        2,
        "Lisbonne is only Alex and Camille"
    );

    let d = structured(&call(&client, "get_day", json!({"date": "2026-07-14"})).await);
    assert_eq!(d["weekday"], "tuesday");
    assert_eq!(d["public_holiday"], "Fête nationale");
    assert_eq!(d["is_weekend"], false);
    assert_eq!(d["daycare_closed"], false);
    let d = structured(&call(&client, "get_day", json!({"date": "2026-08-04"})).await);
    assert_eq!(d["daycare_closed"], true);
    assert_eq!(d["school_holiday"], "Vacances d’été");
    assert!(
        d["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["title"] == "L'été à la maison de Normandie")
    );
    let d = structured(&call(&client, "get_day", json!({"date": "2026-03-12"})).await);
    assert_eq!(d["birthdays"][0]["title"], "Anniversaire de Sacha");

    let f = structured(
        &call(
            &client,
            "find_free_periods",
            json!({"from": "2026-07-01", "to": "2026-07-31", "min_days": 3}),
        )
        .await,
    );
    let periods = f["periods"].as_array().unwrap();
    assert_eq!(
        periods[0],
        json!({"start": "2026-07-01", "end": "2026-07-16", "days": 16})
    );
    assert!(periods.iter().all(|p| p["days"].as_u64().unwrap() >= 3));
    let f = structured(
        &call(
            &client,
            "find_free_periods",
            json!({"from": "2026-07-01", "to": "2026-08-31", "during": "outside_school_holidays"}),
        )
        .await,
    );
    assert_eq!(
        f["periods"],
        json!([{"start": "2026-07-01", "end": "2026-07-03", "days": 3}])
    );

    client.cancel().await.ok();
}

#[tokio::test]
async fn tool_input_errors_are_french_tool_errors() {
    let (state, base) = setup().await;
    let client = connect(&base, &pat(&state, "Agent", Scope::ReadWrite).await).await;
    let r = call(
        &client,
        "list_events",
        json!({"from": "2026-01-01", "to": "2027-12-31"}),
    )
    .await;
    assert_eq!(r.is_error, Some(true));
    assert_eq!(structured(&r)["code"], "invalid_params");
    assert!(
        structured(&r)["message"]
            .as_str()
            .unwrap()
            .contains("400 jours")
    );
    let r = call(&client, "get_day", json!({"date": "demain"})).await;
    assert_eq!(r.is_error, Some(true));
    assert!(
        structured(&r)["message"]
            .as_str()
            .unwrap()
            .contains("Date invalide")
    );
    let r = call(
        &client,
        "find_free_periods",
        json!({"from": "2026-01-01", "to": "2026-01-31", "members": ["Papi"]}),
    )
    .await;
    assert_eq!(structured(&r)["message"], "Personne inconnue : Papi.");
    let r = call(
        &client,
        "create_event",
        json!({"type": "trip", "title": " ", "start": "2026-08-10"}),
    )
    .await;
    assert_eq!(r.is_error, Some(true));
    assert_eq!(
        structured(&r),
        json!({"code": "validation_failed", "message": "Donnez un titre à l’événement."})
    );
    let r = call(&client, "delete_event", json!({"id": "f:inconnu"})).await;
    assert_eq!(structured(&r)["code"], "not_found");
    client.cancel().await.ok();
}

#[tokio::test]
async fn write_tools_record_the_agent_name_and_edit_the_calendar() {
    let (state, base) = setup().await;
    let client = connect(&base, &pat(&state, "Grok API", Scope::ReadWrite).await).await;

    let created = structured(&call(&client, "create_event", json!({
        "type": "trip", "title": "Week-end à Nantes", "start": "2026-11-20", "end": "2026-11-22",
        "members": ["Alex", "Camille"], "location": "Nantes"
    })).await);
    assert_eq!(created["created_by"], "Grok API");
    assert_eq!(created["end"], "2026-11-22");
    assert_eq!(created["calendar"], "famille");
    let id = created["id"].as_str().unwrap().to_string();

    // The web API sees it too.
    let mut app = TestApp::with_state(state.clone());
    app.login().await;
    let v = app.get("/api/events?year=2026").await.json();
    assert!(
        v["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["id"] == id.as_str())
    );

    let updated = structured(
        &call(
            &client,
            "update_event",
            json!({"id": id, "title": "Nantes !", "location": null, "end": "2026-11-23"}),
        )
        .await,
    );
    assert_eq!(updated["title"], "Nantes !");
    assert_eq!(updated["location"], Value::Null);
    assert_eq!(updated["end"], "2026-11-23");
    assert_eq!(updated["updated_by"], "Grok API");
    assert_eq!(
        updated["members"],
        json!(["Alex", "Camille"]),
        "unchanged fields stay"
    );

    let b = structured(&call(&client, "create_event", json!({"type": "birthday", "title": "Anniversaire de Léa", "start": "2026-05-05", "birth_year": 2020})).await);
    assert_eq!(b["age"], 6);
    let d = structured(&call(&client, "delete_event", json!({"id": b["id"]})).await);
    assert_eq!(
        d,
        json!({"deleted": true, "id": b["id"], "scope": "series"})
    );
    let d = structured(&call(&client, "delete_event", json!({"id": id})).await);
    assert_eq!(d["scope"], "event");

    // Imported events are read-only for agents too.
    let holidays = structured(
        &call(
            &client,
            "list_events",
            json!({"from": "2026-07-14", "to": "2026-07-14", "types": ["public_holiday"]}),
        )
        .await,
    );
    let hid = holidays["events"][0]["id"].clone();
    let r = call(&client, "update_event", json!({"id": hid, "title": "x"})).await;
    assert_eq!(structured(&r)["code"], "read_only");
    let r = call(&client, "delete_event", json!({"id": hid})).await;
    assert_eq!(structured(&r)["code"], "read_only");
    client.cancel().await.ok();
}

#[tokio::test]
async fn read_only_tokens_cannot_write() {
    let (state, base) = setup().await;
    let client = connect(&base, &pat(&state, "Lecteur", Scope::Read).await).await;
    for (tool, args) in [
        (
            "create_event",
            json!({"type": "trip", "title": "x", "start": "2026-11-20"}),
        ),
        ("update_event", json!({"id": "f:abc", "title": "x"})),
        ("delete_event", json!({"id": "f:abc"})),
    ] {
        let r = call(&client, tool, args).await;
        assert_eq!(r.is_error, Some(true), "{tool}");
        assert_eq!(
            structured(&r),
            json!({"code": "insufficient_scope", "message": "Ce jeton ne permet que la lecture."})
        );
    }
    let evs = state.store.list_year(2026).await.unwrap();
    assert!(evs.iter().all(|e| e.title != "x"));
    client.cancel().await.ok();
}

#[tokio::test]
async fn mcp_is_rate_limited_per_token() {
    let (state, base) = setup().await;
    let a = pat(&state, "A", Scope::Read).await;
    let b = pat(&state, "B", Scope::Read).await;
    let http = reqwest::Client::new();
    let ping = |tok: &str| {
        http.post(format!("{base}/mcp"))
            .header("authorization", format!("Bearer {tok}"))
            .header("content-type", "application/json")
            .header("accept", "application/json, text/event-stream")
            .body(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#)
            .send()
    };
    let mut first_limited = None;
    for i in 0..130 {
        if ping(&a).await.unwrap().status() == 429 {
            first_limited = Some(i);
            break;
        }
    }
    assert_eq!(first_limited, Some(120));
    assert_ne!(
        ping(&b).await.unwrap().status(),
        429,
        "limits are per token"
    );
}

// ---- Réglages API: token management --------------------------------------------------------

#[tokio::test]
async fn token_admin_api() {
    let mut app = TestApp::demo().await;
    assert_eq!(
        app.get("/api/admin/tokens").await.status,
        StatusCode::UNAUTHORIZED
    );
    app.login().await;
    assert_eq!(app.get("/api/admin/tokens").await.json(), json!([]));

    let r = app
        .post(
            "/api/admin/tokens",
            json!({"name": "Grok API", "scope": "read_write"}),
        )
        .await;
    assert_eq!(r.status, StatusCode::CREATED);
    let created = r.json();
    let clear = created["token"].as_str().unwrap().to_string();
    assert!(clear.starts_with("fy_pat_"));
    assert!(created.get("kiosk_url").is_none());

    // The token is shown once: listings never contain it.
    let list = app.get("/api/admin/tokens").await;
    assert!(!list.text().contains(&clear));
    let j = list.json();
    assert_eq!(j[0]["name"], "Grok API");
    assert_eq!(j[0]["scope"], "read_write");
    assert_eq!(j[0]["kiosk"], false);
    assert_eq!(j[0]["last_used_at"], Value::Null);
    assert!(j[0]["created_at"].is_string());
    assert!(j[0].get("token").is_none() && j[0].get("token_hash").is_none());

    // Duplicate and invalid names.
    let r = app
        .post("/api/admin/tokens", json!({"name": "Grok API"}))
        .await;
    assert_eq!(r.status, StatusCode::CONFLICT);
    assert_eq!(
        r.json()["error"]["message"],
        "Un jeton actif porte déjà ce nom."
    );
    let r = app.post("/api/admin/tokens", json!({"name": "  "})).await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    assert_eq!(r.json()["error"]["message"], "Donnez un nom au jeton.");
    let r = app
        .post("/api/admin/tokens", json!({"name": "x", "scope": "admin"}))
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);

    // The token works on /mcp, then dies on revocation.
    let base = spawn_server(app.state.clone()).await;
    let client = connect(&base, &clear).await;
    assert_eq!(
        structured(&call(&client, "get_context", json!({})).await)["today"],
        "2026-10-16"
    );
    client.cancel().await.ok();
    let used = app.get("/api/admin/tokens").await.json();
    assert!(used[0]["last_used_at"].is_string(), "use is recorded");

    let id = created["id"].as_str().unwrap();
    assert_eq!(
        app.delete(&format!("/api/admin/tokens/{id}")).await.status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        app.delete(&format!("/api/admin/tokens/{id}")).await.status,
        StatusCode::NOT_FOUND
    );
    let r = reqwest::Client::new()
        .post(format!("{base}/mcp"))
        .header("authorization", format!("Bearer {clear}"))
        .header("content-type", "application/json")
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 401);
    assert_eq!(app.get("/api/admin/tokens").await.json(), json!([]));
}

#[tokio::test]
async fn kiosk_links_are_read_only_tokens_with_a_url() {
    let mut app = TestApp::demo().await;
    app.login().await;
    let r = app
        .post(
            "/api/admin/tokens",
            json!({"name": "", "kiosk": true, "scope": "read_write"}),
        )
        .await;
    assert_eq!(r.status, StatusCode::CREATED);
    let j = r.json();
    let clear = j["token"].as_str().unwrap();
    assert_eq!(
        j["kiosk_url"],
        format!("http://localhost:8080/kiosk?token={clear}")
    );
    let list = app.get("/api/admin/tokens").await.json();
    assert_eq!(list[0]["kiosk"], true);
    assert_eq!(list[0]["scope"], "read");
    assert_eq!(list[0]["name"], "Kiosque – 16 octobre");
}

#[tokio::test]
async fn custom_types_appear_in_the_context_and_the_tool_schemas_and_can_be_used() {
    use annee_domain::{EventType, Labels, Names, TypeCatalog, TypeDef};
    let (state, base) = setup().await;
    let client = connect(&base, &pat(&state, "Claude", Scope::ReadWrite).await).await;
    let before = client.list_all_tools().await.unwrap();
    let enum_of = |tools: &[rmcp::model::Tool], tool: &str| {
        let t = tools.iter().find(|t| t.name == tool).unwrap();
        t.input_schema["properties"]["type"]["enum"].clone()
    };
    assert!(
        !enum_of(&before, "create_event")
            .to_string()
            .contains("custom_1")
    );

    let names = |s: &str| Names {
        singular: s.into(),
        plural: format!("{s}s"),
    };
    let mut defs = state.types.current().defs().to_vec();
    defs.push(TypeDef {
        kind: EventType::Custom(1),
        color_id: Some("6".into()),
        labels: Labels {
            fr: names("Sport"),
            en: names("Sport"),
        },
    });
    state
        .type_admin
        .apply(TypeCatalog::new(defs).unwrap())
        .await
        .unwrap();

    let ctx = structured(&call(&client, "get_context", json!({})).await);
    let t = ctx["types"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["code"] == "custom_1")
        .unwrap()
        .clone();
    assert_eq!(t["label"], "Sport");
    assert_eq!(t["labels"], json!({"fr": "Sport", "en": "Sport"}));
    assert_eq!(
        (t["custom"].clone(), t["color_id"].clone()),
        (json!(true), json!("6"))
    );
    let trip = ctx["types"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["code"] == "trip")
        .unwrap()
        .clone();
    assert_eq!(trip["color_id"], "9");

    let after = client.list_all_tools().await.unwrap();
    for tool in ["create_event", "update_event"] {
        assert!(
            enum_of(&after, tool).to_string().contains("custom_1"),
            "{tool}"
        );
    }
    let ev = structured(
        &call(
            &client,
            "create_event",
            json!({"type": "custom_1", "title": "Judo", "start": "2026-11-04"}),
        )
        .await,
    );
    assert_eq!(ev["type"], "custom_1");
    let found = structured(
        &call(
            &client,
            "list_events",
            json!({"from": "2026-11-01", "to": "2026-11-30", "types": ["custom_1"]}),
        )
        .await,
    );
    assert_eq!(found["events"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn mcp_uses_the_instance_default_language() {
    // rmcp runs each tool call in its own task: the language must survive that hop.
    let mut config = annee_server::config::AppConfig::default();
    config.app.default_language = "en".into();
    let state = AppState::demo(
        config,
        chrono::NaiveDate::from_ymd_opt(2026, 10, 16),
        axum_extra::extract::cookie::Key::generate(),
    )
    .await
    .unwrap();
    let base = spawn_server(state.clone()).await;
    let client = connect(&base, &pat(&state, "Claude", Scope::ReadWrite).await).await;

    let ctx = structured(&call(&client, "get_context", json!({})).await);
    let trip = ctx["types"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["code"] == "trip")
        .unwrap()
        .clone();
    assert_eq!(trip["label"], "Trip or vacation");

    let r = call(
        &client,
        "create_event",
        json!({"type": "trip", "title": " ", "start": "2026-07-01"}),
    )
    .await;
    assert_eq!(r.is_error, Some(true));
    assert_eq!(structured(&r)["message"], "Give the event a title.");

    let found = structured(
        &call(
            &client,
            "list_events",
            json!({"from": "2026-07-14", "to": "2026-07-14", "types": ["public_holiday"]}),
        )
        .await,
    );
    assert_eq!(found["events"][0]["title"], "Bastille Day");
}
