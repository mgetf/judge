use std::convert::Infallible;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Form, Router};
use futures_util::stream::unfold;
use maud::Markup;
use serde::Deserialize;
use tower_http::trace::TraceLayer;

use crate::action::EvalForm;
use crate::app::{commit, sync_roster, AppError, AppState};
use crate::bot::{self, current_docket_view};
use crate::clock::now_ms;
use crate::events::Event;
use crate::html;
use crate::ids::{CaseId, PolicyId, PrincipalId};
use crate::session::{
    self, cookie_from_headers, decode, encode, principal_from_headers, random_state,
    OAUTH_STATE_COOKIE, SESSION_COOKIE,
};
use crate::state::{Fold, Principal};
use crate::view::{see_case, see_docket, Target};

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(docket))
        .route("/see", get(see))
        .route("/eval", post(eval))
        .route("/live", get(live))
        .route("/login", get(login))
        .route("/logout", get(logout))
        .route("/auth/discord/callback", get(oauth_callback))
        .route("/discord/interactions", post(discord_interactions))
        .route("/cases", post(eval))
        .route("/cases/{id}", get(show_case))
        .route("/cases/{id}/transcript", get(show_transcript))
        .route("/cases/{id}/evidence", post(eval))
        .route("/cases/{id}/outcomes", post(eval))
        .route("/cases/{id}/notify", post(eval_notify))
        .route("/cases/{id}/respond", post(eval))
        .route("/cases/{id}/deliberate", post(eval_deliberate))
        .route("/cases/{id}/vote", post(eval))
        .route("/cases/{id}/close", post(eval_close))
        .route("/people/{id}", get(show_person))
        .route("/policies/{*id}", get(show_policy))
        .route("/log", get(show_log))
        .route("/sync", post(manual_sync))
        .route("/healthz", get(|| async { "ok" }))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

fn html_ok(body: Markup) -> Response {
    body.into_response()
}

async fn viewer(state: &AppState, headers: &HeaderMap) -> Option<Principal> {
    let id = principal_from_headers(headers, &state.session_secret)?;
    let g = state.gov.read().await;
    g.principals.get(&id).cloned()
}

fn redirect_see(path: &str) -> Response {
    Redirect::to(path).into_response()
}

fn redirect_with_session(
    path: &str,
    secret: &str,
    principal: &PrincipalId,
    secure: bool,
) -> Response {
    let cookie = session::set_cookie(
        SESSION_COOKIE,
        &encode(secret, principal.as_str()),
        60 * 60 * 24 * 30,
        secure,
    );
    (
        StatusCode::SEE_OTHER,
        [
            (header::LOCATION, path.to_string()),
            (header::SET_COOKIE, cookie),
        ],
    )
        .into_response()
}

async fn docket(State(state): State<AppState>, headers: HeaderMap) -> Response {
    see_target(&state, &headers, Target::Docket, None).await
}

#[derive(Deserialize)]
struct SeeQ {
    cite: Option<String>,
}

async fn see(State(state): State<AppState>, headers: HeaderMap, Query(q): Query<SeeQ>) -> Response {
    let target = parse_cite(q.cite.as_deref().unwrap_or("docket"));
    see_target(&state, &headers, target, None).await
}

fn parse_cite(s: &str) -> Target {
    if s.is_empty() || s == "docket" {
        return Target::Docket;
    }
    if let Some(id) = s.strip_prefix("case:") {
        if let Ok(id) = CaseId::parse(id) {
            return Target::Cite(crate::links::Cite::Case { id });
        }
    }
    Target::Docket
}

async fn see_target(
    state: &AppState,
    headers: &HeaderMap,
    target: Target,
    notice: Option<&str>,
) -> Response {
    let who = viewer(state, headers).await;
    match target {
        Target::Docket => {
            let mut view = current_docket_view(state, notice).await;
            let g = state.gov.read().await;
            let mut cases: Vec<_> = g.cases.values().collect();
            cases.sort_by(|a, b| b.opened_ts.cmp(&a.opened_ts));
            let mut bench: Vec<_> = g
                .principals
                .values()
                .filter(|p| p.seat.is_some())
                .cloned()
                .collect();
            bench.sort_by(|a, b| a.id.cmp(&b.id));
            view = see_docket(
                who.as_ref(),
                &cases,
                &bench,
                notice,
                view.channel_url.clone(),
            );
            html_ok(html::render_view(who.as_ref(), &view))
        }
        Target::Cite(crate::links::Cite::Case { id }) => {
            let g = state.gov.read().await;
            let Some(case) = g.cases.get(&id) else {
                return AppError::NotFound.into_response();
            };
            let channel_url = {
                let b = state.bindings.read().await;
                b.cases
                    .get(id.as_str())
                    .map(|c| bot::channel_link(&state.config.guild_id, &c.channel_id))
            };
            let view = see_case(who.as_ref(), case, &g.principals, notice, channel_url);
            html_ok(html::render_view(who.as_ref(), &view))
        }
        Target::Cite(_) => AppError::NotFound.into_response(),
    }
}

async fn eval(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(mut form): Form<EvalForm>,
) -> Response {
    if form.action.is_empty() {
        if !form.brief.is_empty() {
            form.action = "open_case".into();
        } else if !form.label.is_empty() {
            form.action = "file_evidence".into();
        } else if !form.reason.is_empty() {
            form.action = "vote".into();
        } else if !form.body.is_empty() && !form.id.is_empty() {
            form.action = "propose_outcome".into();
        } else if !form.body.is_empty() {
            form.action = "respond".into();
        }
    }
    eval_form(&state, &headers, form).await
}

async fn eval_notify(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(case): Path<String>,
) -> Response {
    eval_form(
        &state,
        &headers,
        EvalForm {
            action: "notify".into(),
            case,
            ..EvalForm::default()
        },
    )
    .await
}

async fn eval_deliberate(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(case): Path<String>,
) -> Response {
    eval_form(
        &state,
        &headers,
        EvalForm {
            action: "deliberate".into(),
            case,
            ..EvalForm::default()
        },
    )
    .await
}

async fn eval_close(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(case): Path<String>,
) -> Response {
    eval_form(
        &state,
        &headers,
        EvalForm {
            action: "close".into(),
            case,
            ..EvalForm::default()
        },
    )
    .await
}

async fn eval_form(state: &AppState, headers: &HeaderMap, form: EvalForm) -> Response {
    let who = match require_viewer(state, headers).await {
        Ok(p) => p,
        Err(r) => return r,
    };
    let action = match form.parse() {
        Ok(a) => a,
        Err(e) => return html_ok(html::flash_page(Some(&who), &e)),
    };
    let cite = action.cite();
    match commit(state, action.into_event(who.id.clone(), now_ms())).await {
        Ok(()) => redirect_see(&cite.path()),
        Err(e) => html_ok(html::flash_page(Some(&who), &e.to_string())),
    }
}

async fn live(
    State(state): State<AppState>,
) -> Sse<impl futures_util::Stream<Item = Result<SseEvent, Infallible>>> {
    let rx = state.live.subscribe();
    let stream = unfold(rx, |mut rx| async move {
        match rx.recv().await {
            Ok(seq) => Some((
                Ok(SseEvent::default().event("commit").data(seq.to_string())),
                rx,
            )),
            Err(tokio::sync::broadcast::error::RecvError::Closed) => None,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                Some((Ok(SseEvent::default().event("commit").data("lag")), rx))
            }
        }
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

async fn discord_interactions(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Some(pk) = state.discord.env().public_key.as_deref() {
        let ts = headers
            .get("X-Signature-Timestamp")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let sig = headers
            .get("X-Signature-Ed25519")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if !bot::verify_signature(pk, ts, &body, sig) {
            return (StatusCode::UNAUTHORIZED, "bad signature").into_response();
        }
    }
    let parsed: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    };
    match bot::handle_interaction(&state, &parsed).await {
        Ok(v) => (StatusCode::OK, axum::Json(v)).into_response(),
        Err(e) => {
            tracing::warn!("interaction: {e}");
            (
                StatusCode::OK,
                axum::Json(serde_json::json!({
                    "type": 4,
                    "data": { "content": e.to_string(), "flags": 64 }
                })),
            )
                .into_response()
        }
    }
}

async fn login(State(state): State<AppState>) -> Response {
    let st = random_state();
    let loc = state.discord.env().authorize_redirect(&st);
    let cookie = session::set_cookie(
        OAUTH_STATE_COOKIE,
        &encode(&state.session_secret, &st),
        600,
        state.secure_cookies(),
    );
    (
        StatusCode::SEE_OTHER,
        [(header::LOCATION, loc), (header::SET_COOKIE, cookie)],
    )
        .into_response()
}

async fn logout(State(state): State<AppState>) -> Response {
    (
        StatusCode::SEE_OTHER,
        [
            (header::LOCATION, "/".to_string()),
            (
                header::SET_COOKIE,
                session::clear_cookie(SESSION_COOKIE, state.secure_cookies()),
            ),
        ],
    )
        .into_response()
}

#[derive(Deserialize)]
struct OauthQ {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

async fn oauth_callback(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<OauthQ>,
) -> Response {
    if let Some(err) = q.error {
        return html_ok(html::login_required(&format!("Discord error: {err}")));
    }
    let Some(code) = q.code else {
        return html_ok(html::login_required("missing code"));
    };
    let stored = cookie_from_headers(&headers, OAUTH_STATE_COOKIE)
        .and_then(|c| decode(&state.session_secret, &c));
    if stored.as_deref() != q.state.as_deref() {
        return html_ok(html::login_required("oauth state mismatch"));
    }
    let token = match state.discord.exchange_code(&code).await {
        Ok(t) => t,
        Err(e) => return html_ok(html::login_required(&e.to_string())),
    };
    let user = match state.discord.me(&token).await {
        Ok(u) => u,
        Err(e) => return html_ok(html::login_required(&e.to_string())),
    };
    let Ok(pid) = PrincipalId::parse(&user.id) else {
        return html_ok(html::login_required("invalid discord id"));
    };
    if let Err(e) = commit(
        &state,
        Event::PrincipalSeen {
            ts: now_ms(),
            id: pid.clone(),
            display_name: user.display_name(),
        },
    )
    .await
    {
        return html_ok(html::login_required(&e.to_string()));
    }
    if let Err(e) = sync_roster(&state).await {
        tracing::warn!("roster sync after login: {e}");
    }
    redirect_with_session("/", &state.session_secret, &pid, state.secure_cookies())
}

async fn require_viewer(state: &AppState, headers: &HeaderMap) -> Result<Principal, Response> {
    viewer(state, headers)
        .await
        .ok_or_else(|| html_ok(html::login_required("Log in to act.")))
}

async fn show_case(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let Ok(cid) = CaseId::parse(&id) else {
        return AppError::NotFound.into_response();
    };
    see_target(
        &state,
        &headers,
        Target::Cite(crate::links::Cite::Case { id: cid }),
        None,
    )
    .await
}

async fn show_person(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let who = viewer(&state, &headers).await;
    let Ok(pid) = PrincipalId::parse(&id) else {
        return AppError::NotFound.into_response();
    };
    let g = state.gov.read().await;
    let Some(p) = g.principals.get(&pid) else {
        return AppError::NotFound.into_response();
    };
    html_ok(html::person_page(who.as_ref(), p))
}

async fn show_policy(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let who = viewer(&state, &headers).await;
    let Ok(pid) = PolicyId::parse(&id) else {
        return AppError::NotFound.into_response();
    };
    let g = state.gov.read().await;
    let Some(p) = g.policies.get(&pid) else {
        return AppError::NotFound.into_response();
    };
    html_ok(html::policy_page(who.as_ref(), p))
}

async fn show_transcript(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    let Ok(cid) = CaseId::parse(&id) else {
        return AppError::NotFound.into_response();
    };
    let path = bot::transcript_file(&state, &cid);
    match std::fs::read_to_string(path) {
        Ok(html) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            html,
        )
            .into_response(),
        Err(_) => AppError::NotFound.into_response(),
    }
}

async fn show_log(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let who = viewer(&state, &headers).await;
    let g = state.gov.read().await;
    let lines: Vec<_> = g
        .attempts
        .iter()
        .map(|a| {
            let fold = match &a.fold {
                Fold::Accepted => "accepted".to_string(),
                Fold::Rejected(r) => format!("rejected:{r}"),
            };
            let ev = serde_json::to_string(&a.event).unwrap_or_default();
            (a.seq, fold, ev)
        })
        .collect();
    html_ok(html::log_page(who.as_ref(), &lines))
}

async fn manual_sync(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let who = match require_viewer(&state, &headers).await {
        Ok(p) => p,
        Err(r) => return r,
    };
    if !who.is_voting_seat() {
        return html_ok(html::flash_page(Some(&who), "not seated"));
    }
    match sync_roster(&state).await {
        Ok(()) => redirect_see("/"),
        Err(e) => html_ok(html::flash_page(Some(&who), &e.to_string())),
    }
}
