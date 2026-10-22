//! Language of the current request (§19.2).
//!
//! A middleware resolves the language (query `?lang=`, then cookie `annee_lang`, then the
//! instance default) and runs the rest of the request inside a task-local scope, so that error
//! messages, server-rendered pages and imported titles can ask for [`current`] without every
//! function having to carry the language around. Outside a request (CLI, background jobs) the
//! language is French.

use annee_domain::Lang;
use axum::extract::{Request, State};
use axum::http::header::{COOKIE, HeaderMap, HeaderValue, SET_COOKIE};
use axum::middleware::Next;
use axum::response::Response;

use crate::state::AppState;

pub const LANG_COOKIE: &str = "annee_lang";
const COOKIE_SECONDS: u32 = 365 * 24 * 3600;

tokio::task_local! {
    static LANG: Lang;
}

/// The language of the request being handled (French outside a request).
pub fn current() -> Lang {
    LANG.try_with(|l| *l).unwrap_or_default()
}

/// Runs `fut` as if it handled a request in `lang`. Code spawned off the request (rmcp runs each
/// MCP call in its own task) loses the task-local, so it re-enters the scope with the language
/// resolved at the HTTP layer.
pub async fn scoped<F: std::future::Future>(lang: Lang, fut: F) -> F::Output {
    LANG.scope(lang, fut).await
}

/// Picks the French or the English text for the current request.
pub fn tr(fr: &str, en: &str) -> String {
    current().pick(fr, en).to_string()
}

fn cookie_lang(headers: &HeaderMap) -> Option<Lang> {
    headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|kv| kv.trim().split_once('='))
        .find(|(k, _)| *k == LANG_COOKIE)
        .and_then(|(_, v)| Lang::parse(v))
}

fn query_lang(query: Option<&str>) -> Option<Lang> {
    query?
        .split('&')
        .filter_map(|kv| kv.split_once('='))
        .find(|(k, _)| *k == "lang")
        .and_then(|(_, v)| Lang::parse(v))
}

/// Resolve the language of a request: explicit `?lang=`, then cookie, then `default`.
pub fn resolve(headers: &HeaderMap, query: Option<&str>, default: Lang) -> Lang {
    query_lang(query)
        .or_else(|| cookie_lang(headers))
        .unwrap_or(default)
}

pub fn lang_cookie_header(lang: Lang, secure: bool) -> Option<HeaderValue> {
    let secure = if secure { "; Secure" } else { "" };
    HeaderValue::from_str(&format!(
        "{LANG_COOKIE}={}; Path=/; Max-Age={COOKIE_SECONDS}; SameSite=Lax{secure}",
        lang.code()
    ))
    .ok()
}

/// Middleware: scope the request to its language and persist an explicit `?lang=` choice.
pub async fn lang_layer(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let query = req.uri().query();
    let explicit = query_lang(query);
    let lang = resolve(req.headers(), query, state.config.default_lang());
    let mut res = LANG.scope(lang, next.run(req)).await;
    if let Some(chosen) = explicit
        && let Some(v) = lang_cookie_header(chosen, state.config.secure_cookies())
    {
        res.headers_mut().append(SET_COOKIE, v);
    }
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_query_beats_cookie_beats_default() {
        let mut h = HeaderMap::new();
        h.insert(COOKIE, HeaderValue::from_static("a=b; annee_lang=en; c=d"));
        assert_eq!(resolve(&h, None, Lang::Fr), Lang::En);
        assert_eq!(resolve(&h, Some("x=1&lang=fr"), Lang::Fr), Lang::Fr);
        assert_eq!(resolve(&HeaderMap::new(), None, Lang::En), Lang::En);
        assert_eq!(
            resolve(&HeaderMap::new(), Some("lang=de"), Lang::Fr),
            Lang::Fr
        );
    }

    #[tokio::test]
    async fn current_is_scoped_and_defaults_to_french() {
        assert_eq!(current(), Lang::Fr);
        let v = LANG.scope(Lang::En, async { tr("bonjour", "hello") }).await;
        assert_eq!(v, "hello");
        assert_eq!(tr("bonjour", "hello"), "bonjour");
    }
}
