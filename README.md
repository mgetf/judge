# judge

An event-sourced court for a Discord community. The log is the history, the
reducer is the constitution: every attempted act is journaled, and legitimacy
means exactly "the reducer accepted it." If an event didn't fold into state, it
didn't happen.

Discord is the courtroom. Each case is a Ticket Tool–shaped ticket: a private
channel under a court category, a pinned live `see` message that the bot edits
in place, slash commands and buttons for every act, a two-step close, and an
HTML transcript when the case ends. A small web app mirrors the same state for
browsing the docket, cases, people, policies, and the raw event log.

## Design stance

- **The log is the history.** Events are appended to a JSONL file
  (`events.jsonl` in `JUDGE_DATA_DIR`). Every attempt is journaled, accepted or
  rejected. Replaying the log rebuilds the entire state.
- **The reducer is the constitution.** `src/reducer.rs` is the only authority
  on what is allowed. There is no side channel.
- **A case is the unit of decision.** Cases move through explicit phases —
  `intake → (noticed) → deliberation → closed / lapsed`, with `vacated` reachable
  only by a successful appeal — and every transition is an event with an actor
  attached.
- **Franchise is a standing bench.** Discord roles map to seats and vote
  weights via `config.json`, imported wholesale from the guild roster and
  snapshotted (frozen) when deliberation opens. A ballot is one outcome plus a
  reason, counted at the voter's frozen weight.
- **Hearing is opt-in.** The default case is a record: document evidence,
  freeze a verdict. Notifying the subject and waiting on a response window is a
  special shape (`hearing: required`), not the shape of every case.
- **AI clerks are first-class but non-sovereign.** Clerks file notes
  (summaries, contradictions, precedent links); they never vote, open, or
  close, and their acts are permanently marked.
- **Everything has a stable link path** (`/cases/{id}`, `/people/{id}`,
  `/policies/{id}`, `/log`), so verdicts and policies are citable.

## Decision kinds and rules

Each case has a `DecisionKind` that sets its quorum and margin
(see `src/types.rs`):

| Kind | Quorum (bench weight) | Winner margin | Notes |
|---|---|---|---|
| `record` | 50% | tie-break only | Document a fact, freeze a verdict (cheats, smurfs, obvious bans) |
| `routine` | 50% | tie-break only | Day-to-day mod action |
| `personnel` | 50% | 1.15× | Hire / fire / demote; 48h response window when a hearing is required |
| `policy` | 50% | 1.10× | Enact or amend standing policy (citable precedent) |
| `constitutional` | 66.7% | 1.50× | Change the rules of the rules |
| `appeal` | 50% | 1.25× | A case about a closed case; verdict `vacate` nullifies the target |

Scoring is weighted plurality (`src/scoring.rs`). Exact ties always lapse. A
case that fails quorum or margin closes as `lapsed`, without a verdict.

## Discord UX

The bot registers guild slash commands and drives the whole flow in Discord
(`src/bot.rs`, `src/ticket.rs`):

- `/case` — open a case (id, brief, kind, hearing, subject, appeal target)
- `/evidence` — attach a file as evidence (id defaults to the filename)
- `/outcome` — propose an outcome (optionally enacting a policy)
- `/vote` — cast a ballot (outcome + reason)
- `/notify`, `/deliberate` — advance the phase
- `/close` — two-step close: ask, then confirm by button
- `/add`, `/remove` — manage who can see the ticket
- `/transcript` — save an HTML transcript of the channel
- `/docket` — show the live docket

Opening a case creates a private channel, posts and pins a live `see` view,
and updates the docket message. Closing moves the channel under the closed
category, renames it, and posts the transcript. Channel/message bindings are
persisted in `JUDGE_DATA_DIR` so they survive restarts.

## Web app

An axum server (`src/http.rs`) serves the same state over HTTP with Discord
OAuth login:

- `/` — the docket
- `/cases/{id}`, `/cases/{id}/transcript` — case detail and transcript
- `/people/{id}`, `/policies/{id}`, `/log` — principals, policies, raw log
- `/see`, `/live`, `/eval` — live-updating views and the action endpoint
- `/discord/interactions` — Discord interactions webhook (Ed25519-verified)
- `/healthz` — health check

## Running locally

Requires Rust (see `rust-toolchain.toml`).

```sh
cp .env.example .env      # fill in Discord credentials, or use the mock below
cp config.example.json config.json   # map your guild's role ids to seats/weights
cargo run
```

`config.json` binds Discord role ids to seats and weights:

```json
{
  "guild_id": "…",
  "owner_discord_id": "…",
  "roles": {
    "role-id": { "seat": "chief", "weight": 3 },
    "role-id": { "seat": "justice", "weight": 1 },
    "role-id": { "seat": "clerk", "weight": 0 }
  }
}
```

### Environment

| Variable | Purpose |
|---|---|
| `JUDGE_BIND` | Listen address (default `127.0.0.1:8080`) |
| `JUDGE_PUBLIC_URL` | Public base URL; `https://` enables secure cookies |
| `JUDGE_DATA_DIR` | Directory for `events.jsonl` and Discord bindings |
| `JUDGE_CONFIG` | Path to `config.json` |
| `JUDGE_SESSION_SECRET` | HMAC secret for session cookies |
| `DISCORD_CLIENT_ID` / `DISCORD_CLIENT_SECRET` | OAuth app credentials |
| `DISCORD_REDIRECT_URI` | Defaults to `${JUDGE_PUBLIC_URL}/auth/discord/callback` |
| `DISCORD_BOT_TOKEN` | Bot token; unset disables the Discord-side UX |
| `DISCORD_PUBLIC_KEY` / `DISCORD_APPLICATION_ID` | Interactions signature check; empty key skips verification (dev/mock) |
| `DISCORD_API_BASE` / `DISCORD_AUTHORIZE_URL` | Override to point at the mock Discord |

### Mock Discord

`src/mock_discord.rs` is an in-process Discord: OAuth, the REST endpoints the
judge uses, and a browsable guild UI where chat is a channel and buttons and
attachments are native. Run it standalone:

```sh
cargo run --bin mock-discord
# then point the judge at the printed DISCORD_AUTHORIZE_URL / DISCORD_API_BASE
```

`MOCK_DISCORD_USERS_JSON`, `MOCK_DISCORD_CLIENT_ID`,
`MOCK_DISCORD_CLIENT_SECRET`, and `MOCK_DISCORD_GUILD_ID` configure it.

## Tests

```sh
cargo test
```

Integration tests (`tests/`) spin up the judge plus the mock Discord in one
process and exercise the full flow: OAuth login and roster import
(`mock_oauth.rs`), the case-channel UX (`discord_ux.rs`), and a browser-driven
end-to-end run through the mock guild UI (`playwright.rs`, which needs
Chromium: `npx playwright@1.61.1 install chromium`). Reducer unit tests live in
`src/reducer/tests.rs`.

## Deployment

Pushes to `main` deploy to Fly.io via `.github/workflows/deploy.yml`
(needs the `FLY_API_TOKEN` secret). `fly.toml` mounts a `judge_data` volume at
`/data` for the event log, config, and Discord bindings, so the court's history
survives deploys. The `Dockerfile` builds a slim release image.

## Repository layout

| Path | What it is |
|---|---|
| `src/` | The crate: reducer, events, state, Discord bot + client, web app, mock Discord |
| `src/reducer.rs` | The constitution — the only way state changes |
| `src/events.rs` / `src/event_log.rs` | Event types and the durable JSONL log |
| `src/bot.rs` / `src/ticket.rs` | Discord-side UX: case channels, live views, transcripts |
| `src/mock_discord.rs` | In-process Discord for dev and tests |
| `tests/` | Integration tests against the judge + mock Discord stack |
| `library.tdsl`, `seed.tdsl` | Design notes and seed docket |
| `from-thousand/`, root `*.rs` files | Reference material from a prior project; not part of the build |
