use axum::extract::{DefaultBodyLimit, Multipart, Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Form, Router};
use maud::Markup;
use serde::Deserialize;
use tower_http::trace::TraceLayer;

use crate::blob::{self, MAX_BYTES};

use crate::app::{commit, sync_roster, AppError, AppState};
use crate::clock::now_ms;
use crate::events::Event;
use crate::html;
use crate::ids::{CaseId, EvidenceId, NoteId, OutcomeId, PolicyId, PrincipalId};
use crate::session::{
    self, cookie_from_headers, decode, encode, principal_from_headers, random_state,
    OAUTH_STATE_COOKIE, SESSION_COOKIE,
};
use crate::state::{Fold, Principal};
use crate::types::{DecisionKind, Hearing};

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(docket))
        .route("/login", get(login))
        .route("/logout", get(logout))
        .route("/auth/discord/callback", get(oauth_callback))
        .route("/cases", post(open_case))
        .route("/cases/{id}", get(show_case))
        .route("/cases/{id}/evidence", post(file_evidence))
        .route("/cases/{id}/outcomes", post(propose_outcome))
        .route("/cases/{id}/notify", post(notify_subject))
        .route("/cases/{id}/respond", post(respond))
        .route("/cases/{id}/deliberate", post(deliberate))
        .route("/cases/{id}/vote", post(vote))
        .route("/cases/{id}/close", post(close_case))
        .route("/people/{id}", get(show_person))
        .route("/policies/{*id}", get(show_policy))
        .route("/log", get(show_log))
        .route("/sync", post(manual_sync))
        .route("/blobs/{*key}", get(serve_blob))
        .route("/jquery.js", get(jquery))
        .layer(DefaultBodyLimit::max(MAX_BYTES + 1024 * 1024))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn jquery() -> impl IntoResponse {
    (
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/javascript; charset=utf-8"),
        )],
        include_str!("../static/jquery-3.7.1.min.js"),
    )
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

fn redirect_with_session(path: &str, secret: &str, principal: &PrincipalId) -> Response {
    let cookie = session::set_cookie(
        SESSION_COOKIE,
        &encode(secret, principal.as_str()),
        60 * 60 * 24 * 30,
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
    let who = viewer(&state, &headers).await;
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
    html_ok(html::docket(who.as_ref(), &cases, &bench, None))
}

async fn login(State(state): State<AppState>) -> Response {
    let st = random_state();
    let loc = state.discord.env().authorize_redirect(&st);
    let cookie = session::set_cookie(OAUTH_STATE_COOKIE, &encode(&state.session_secret, &st), 600);
    (
        StatusCode::SEE_OTHER,
        [(header::LOCATION, loc), (header::SET_COOKIE, cookie)],
    )
        .into_response()
}

async fn logout() -> Response {
    (
        StatusCode::SEE_OTHER,
        [
            (header::LOCATION, "/".to_string()),
            (header::SET_COOKIE, session::clear_cookie(SESSION_COOKIE)),
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
    redirect_with_session("/", &state.session_secret, &pid)
}

#[derive(Deserialize)]
struct OpenCaseForm {
    id: String,
    kind: String,
    #[serde(default)]
    hearing: String,
    brief: String,
    #[serde(default)]
    subject: String,
    #[serde(default)]
    target_case: String,
}

async fn require_viewer(state: &AppState, headers: &HeaderMap) -> Result<Principal, Response> {
    viewer(state, headers)
        .await
        .ok_or_else(|| html_ok(html::login_required("Log in to act.")))
}

async fn open_case(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<OpenCaseForm>,
) -> Response {
    let who = match require_viewer(&state, &headers).await {
        Ok(p) => p,
        Err(r) => return r,
    };
    let id = match CaseId::parse(form.id.trim()) {
        Ok(id) => id,
        Err(e) => return html_ok(html::flash_page(Some(&who), &e.to_string())),
    };
    let kind = match form.kind.parse::<DecisionKind>() {
        Ok(k) => k,
        Err(()) => return html_ok(html::flash_page(Some(&who), "unknown kind")),
    };
    let hearing = form.hearing.parse::<Hearing>().unwrap_or(Hearing::None);
    let subject = optional_pid(&form.subject);
    let target_case = optional_cid(&form.target_case);
    match commit(
        &state,
        Event::CaseOpened {
            ts: now_ms(),
            id: id.clone(),
            kind,
            hearing,
            opened_by: who.id.clone(),
            brief: form.brief,
            subject,
            target_case,
        },
    )
    .await
    {
        Ok(()) => redirect_see(&format!("/cases/{id}")),
        Err(e) => html_ok(html::flash_page(Some(&who), &e.to_string())),
    }
}

async fn show_case(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let who = viewer(&state, &headers).await;
    let Ok(cid) = CaseId::parse(&id) else {
        return AppError::NotFound.into_response();
    };
    let g = state.gov.read().await;
    let Some(case) = g.cases.get(&cid) else {
        return AppError::NotFound.into_response();
    };
    html_ok(html::case_page(who.as_ref(), case, None, &g.principals))
}

struct UploadedFile {
    filename: String,
    content_type: String,
    bytes: Vec<u8>,
}

struct EvidenceSubmission {
    id: String,
    label: String,
    body: String,
    file: Option<UploadedFile>,
}

async fn file_evidence(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(case): Path<String>,
    multipart: Multipart,
) -> Response {
    let who = match require_viewer(&state, &headers).await {
        Ok(p) => p,
        Err(r) => return r,
    };
    let form = match read_evidence_multipart(multipart).await {
        Ok(f) => f,
        Err(e) => return html_ok(html::flash_page(Some(&who), &e)),
    };
    let mut id = form.id.trim().to_string();
    let mut label = form.label.trim().to_string();
    let mut body = form.body;
    let mut href = None;
    let mut filename = None;
    if let Some(file) = form.file {
        if id.is_empty() {
            id = blob::slug_id(&file.filename);
        }
        if label.is_empty() {
            label = file.filename.clone();
        }
        if body.trim().is_empty() {
            body = file.filename.clone();
        }
        let safe = blob::safe_filename(&file.filename);
        let key = format!("cases/{case}/{id}/{safe}");
        let ct = blob::guess_type(&safe, &file.content_type);
        match state.blobs.put(&key, &file.bytes, &ct).await {
            Ok(url) => {
                href = Some(url);
                filename = Some(safe);
            }
            Err(e) => return html_ok(html::flash_page(Some(&who), &e)),
        }
    }
    let ev = match parse_case_evidence(&case, &id) {
        Ok(ev) => ev,
        Err(e) => return html_ok(html::flash_page(Some(&who), &e)),
    };
    match commit(
        &state,
        Event::EvidenceFiled {
            ts: now_ms(),
            case: ev.0,
            by: who.id.clone(),
            id: ev.1,
            label,
            body,
            href,
            filename,
        },
    )
    .await
    {
        Ok(()) => redirect_see(&format!("/cases/{case}")),
        Err(e) => html_ok(html::flash_page(Some(&who), &e.to_string())),
    }
}

fn parse_case_evidence(case: &str, id: &str) -> Result<(CaseId, EvidenceId), String> {
    Ok((
        CaseId::parse(case).map_err(|e| e.to_string())?,
        EvidenceId::parse(id.trim()).map_err(|e| e.to_string())?,
    ))
}

async fn read_evidence_multipart(mut multipart: Multipart) -> Result<EvidenceSubmission, String> {
    let mut id = String::new();
    let mut label = String::new();
    let mut body = String::new();
    let mut file = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| format!("multipart: {e}"))?
    {
        let name = field.name().unwrap_or("").to_string();
        if name == "file" {
            let filename = field.file_name().unwrap_or("").to_string();
            let content_type = field
                .content_type()
                .unwrap_or("application/octet-stream")
                .to_string();
            let bytes = field.bytes().await.map_err(|e| format!("read file: {e}"))?;
            if !filename.is_empty() && !bytes.is_empty() {
                if bytes.len() > MAX_BYTES {
                    return Err(format!("file larger than {MAX_BYTES} bytes"));
                }
                file = Some(UploadedFile {
                    filename,
                    content_type,
                    bytes: bytes.to_vec(),
                });
            }
        } else {
            let text = field.text().await.map_err(|e| format!("read field: {e}"))?;
            match name.as_str() {
                "id" => id = text,
                "label" => label = text,
                "body" => body = text,
                _ => {}
            }
        }
    }
    Ok(EvidenceSubmission {
        id,
        label,
        body,
        file,
    })
}

async fn serve_blob(State(state): State<AppState>, Path(key): Path<String>) -> Response {
    let Some((bytes, ct)) = state.blobs.get_local(&key) else {
        return AppError::NotFound.into_response();
    };
    (
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_str(&ct).unwrap_or(HeaderValue::from_static("application/octet-stream")),
        )],
        bytes,
    )
        .into_response()
}

#[derive(Deserialize)]
struct OutcomeForm {
    id: String,
    body: String,
    #[serde(default)]
    enacts_policy: String,
}

async fn propose_outcome(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(case): Path<String>,
    Form(form): Form<OutcomeForm>,
) -> Response {
    let who = match require_viewer(&state, &headers).await {
        Ok(p) => p,
        Err(r) => return r,
    };
    let cid = match CaseId::parse(&case) {
        Ok(c) => c,
        Err(e) => return html_ok(html::flash_page(Some(&who), &e.to_string())),
    };
    let oid = match OutcomeId::parse(form.id.trim()) {
        Ok(o) => o,
        Err(e) => return html_ok(html::flash_page(Some(&who), &e.to_string())),
    };
    let policy = if form.enacts_policy.trim().is_empty() {
        None
    } else {
        match PolicyId::parse(form.enacts_policy.trim()) {
            Ok(p) => Some(p),
            Err(e) => return html_ok(html::flash_page(Some(&who), &e.to_string())),
        }
    };
    match commit(
        &state,
        Event::OutcomeProposed {
            ts: now_ms(),
            case: cid,
            by: who.id.clone(),
            id: oid,
            body: form.body,
            enacts_policy: policy,
        },
    )
    .await
    {
        Ok(()) => redirect_see(&format!("/cases/{case}")),
        Err(e) => html_ok(html::flash_page(Some(&who), &e.to_string())),
    }
}

async fn notify_subject(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(case): Path<String>,
) -> Response {
    actor_case_event(&state, &headers, &case, |who, cid| Event::SubjectNotified {
        ts: now_ms(),
        case: cid,
        by: who,
    })
    .await
}

async fn deliberate(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(case): Path<String>,
) -> Response {
    actor_case_event(&state, &headers, &case, |who, cid| {
        Event::DeliberationOpened {
            ts: now_ms(),
            case: cid,
            by: who,
        }
    })
    .await
}

async fn close_case(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(case): Path<String>,
) -> Response {
    actor_case_event(&state, &headers, &case, |who, cid| Event::CaseClosed {
        ts: now_ms(),
        case: cid,
        by: who,
    })
    .await
}

async fn actor_case_event<F>(state: &AppState, headers: &HeaderMap, case: &str, f: F) -> Response
where
    F: FnOnce(PrincipalId, CaseId) -> Event,
{
    let who = match require_viewer(state, headers).await {
        Ok(p) => p,
        Err(r) => return r,
    };
    let cid = match CaseId::parse(case) {
        Ok(c) => c,
        Err(e) => return html_ok(html::flash_page(Some(&who), &e.to_string())),
    };
    match commit(state, f(who.id.clone(), cid)).await {
        Ok(()) => redirect_see(&format!("/cases/{case}")),
        Err(e) => html_ok(html::flash_page(Some(&who), &e.to_string())),
    }
}

#[derive(Deserialize)]
struct RespondForm {
    body: String,
}

async fn respond(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(case): Path<String>,
    Form(form): Form<RespondForm>,
) -> Response {
    let who = match require_viewer(&state, &headers).await {
        Ok(p) => p,
        Err(r) => return r,
    };
    let cid = match CaseId::parse(&case) {
        Ok(c) => c,
        Err(e) => return html_ok(html::flash_page(Some(&who), &e.to_string())),
    };
    match commit(
        &state,
        Event::ResponseFiled {
            ts: now_ms(),
            case: cid,
            by: who.id.clone(),
            body: form.body,
        },
    )
    .await
    {
        Ok(()) => redirect_see(&format!("/cases/{case}")),
        Err(e) => html_ok(html::flash_page(Some(&who), &e.to_string())),
    }
}

#[derive(Deserialize)]
struct VoteForm {
    outcome: String,
    reason: String,
}

async fn vote(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(case): Path<String>,
    Form(form): Form<VoteForm>,
) -> Response {
    let who = match require_viewer(&state, &headers).await {
        Ok(p) => p,
        Err(r) => return r,
    };
    let cid = match CaseId::parse(&case) {
        Ok(c) => c,
        Err(e) => return html_ok(html::flash_page(Some(&who), &e.to_string())),
    };
    let oid = match OutcomeId::parse(form.outcome.trim()) {
        Ok(o) => o,
        Err(e) => return html_ok(html::flash_page(Some(&who), &e.to_string())),
    };
    match commit(
        &state,
        Event::VoteCast {
            ts: now_ms(),
            case: cid,
            voter: who.id.clone(),
            outcome: oid,
            reason: form.reason,
        },
    )
    .await
    {
        Ok(()) => redirect_see(&format!("/cases/{case}")),
        Err(e) => html_ok(html::flash_page(Some(&who), &e.to_string())),
    }
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

fn optional_pid(s: &str) -> Option<PrincipalId> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    PrincipalId::parse(s).ok()
}

fn optional_cid(s: &str) -> Option<CaseId> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    CaseId::parse(s).ok()
}

#[allow(dead_code)]
fn _note_id(s: &str) -> Result<NoteId, String> {
    NoteId::parse(s).map_err(|e| e.to_string())
}
