# The court, as an abstract machine

The judge is a CESK-shaped machine. The **C**ontrol string is the event at the
head of the log. The **E**nvironment is the franchise ρ — who is bound to which
seat at what weight. The **S**tore is the docket σ — cases and policies. The
**K**ontinuation is the unconsumed suffix of the log. There is one extension:
a write-only output register J, the attempt journal. Strictly it is a CESK+J
machine, because this court records its failures.

The machine is **total**: no event gets stuck. An act the constitution refuses
does not halt the machine; it reduces to the same ρ and σ with the refusal
journaled. Failure is an answer, not an absence of one.

Everything below is transcribed from `src/reducer.rs`, `src/scoring.rs`, and
`src/types.rs`; the tables are `rules_for`, the metafunctions are the private
helpers, and each fold rule is one arm of `validate_and_fold`.

## Syntax

```
t ∈ Ts            timestamps (ms)           p, q ∈ Principal   principal ids
c ∈ Case          case ids                  o ∈ Outcome        outcome ids
e ∈ Evidence      evidence ids              n ∈ Note           note ids
π ∈ Policy        policy ids                w ∈ ℕ              vote weights

s   ::= chief | justice | clerk(model?)                        (seats)
k   ::= record | routine | personnel | policy | constitutional | appeal
h   ::= none | required                                        (hearing)
φ   ::= intake | noticed | delib | closed | lapsed | vacated   (phases)

ev  ::= seen(t, p, name)
      | sync(t, ⟨p, name, s, w, roles⟩*)
      | open(t, c, k, h, p, brief, subj?, tgt?)
      | recuse(t, c, q, p)  |  unrecuse(t, c, q, p)
      | evidence(t, c, p, e, label, body)
      | notify(t, c, p)  |  respond(t, c, p, body)
      | propose(t, c, p, o, body, π?)
      | cite(t, c, p, π)
      | deliberate(t, c, p)
      | vote(t, c, p, o, reason)
      | note(t, c, p, n, notekind, body, cites)
      | close(t, c, p)
```

## Machine states

```
ς ::= ⟨K, ρ, σ, J⟩

K ::= ev · K | ε                 the log yet to fold (kontinuation)
ρ  : p ⇀ ⟨name, s?, w, t⟩        the franchise (environment)
σ  = ⟨C, P⟩
C  : c ⇀ ⟨k, h, φ, opener, t₀, brief, subj?, tgt?,
          R,            recusals        (set of p)
          B?,           bench           (frozen snapshot, or ⊥)
          Ev, O, V, N,  evidence, outcomes, ballots, clerk notes
          t_notice?, resp?, cites, verdict?⟩
P  : π ⇀ ⟨versions, cited_in, repealed⟩
J ::= J · ✓ev | J · ✗(ev, r) | ε   the attempt journal (output)
```

A state ⟨ε, ρ, σ, J⟩ is final. The initial state is ⟨K₀, ∅, ⟨∅, ∅⟩, ε⟩ where
K₀ is `events.jsonl`.

## The two transitions

There are exactly two top-level reductions. Every event takes exactly one of
them; which one is decided by the fold judgment ρ, σ ⊢ ev ⇓ ρ′, σ′ and its
complement ρ, σ ⊢ ev ⇑ r.

```
        ρ, σ ⊢ ev ⇓ ρ′, σ′
─────────────────────────────────────        [fold]
⟨ev·K, ρ, σ, J⟩ ↦ ⟨K, ρ′, σ′, J·✓ev⟩

          ρ, σ ⊢ ev ⇑ r
─────────────────────────────────────        [reject]
⟨ev·K, ρ, σ, J⟩ ↦ ⟨K, ρ, σ, J·✗(ev,r)⟩
```

[reject] touches nothing but J. Legitimacy is, by definition, membership of
✓ev in J; there is no other channel into ρ or σ.

## Metafunctions

```
sovereign(ρ, p)   ⇔  p ∈ dom(ρ) ∧ ρ(p).s ∈ {chief, justice} ∧ ρ(p).w > 0
clerk(ρ, p)       ⇔  p ∈ dom(ρ) ∧ ρ(p).s = clerk(_)

bench(ρ, C(c), t) =  ⟨t, [⟨p, ρ(p).s, ρ(p).w⟩ | p ∈ dom(ρ),
                          sovereign(ρ, p), p ∉ R, p ≠ subj ]⟩
                     ordered by p; undefined (⇑ EmptyBench) when empty

sitting(B)        =  Σ ⟨p,s,w⟩ ∈ B . w

tally(O, V)       =  ordering of dom(O) by summed ballot weight, desc,
                     ties by outcome id; unvoted outcomes stay at 0.
                     cast = Σ ballots' weights.

margin(ordering)  =  (∞, ¬tied)   if singleton, or runner-up = 0 with winner > 0
                     (0, ¬tied)   if winner = runner-up = 0
                     (1, tied)    if winner = runner-up > 0
                     (w₁/w₂, ¬tied) otherwise

quorum(cast, sit, f) ⇔  sit > 0 ∧ cast > 0 ∧ cast ≥ sit · f
```

The rules table (`rules_for`), with Q the quorum fraction of sitting bench
weight, M the least winning margin, and W the response window:

```
k               Q      M     W
record          1/2    1.0   —
routine         1/2    1.0   —
personnel       1/2    1.15  48h
policy          1/2    1.10  —
constitutional  2/3    1.5   —
appeal          1/2    1.25  —

window(k) = W(k), defaulting to 48h    (consulted only when h = required)
```

M = 1.0 does not make ties acceptable: `tied` is checked separately, and exact
ties always lapse. M = 1.0 means any strict advantage suffices.

## Fold rules

Throughout, C(c) abbreviates the case record and field updates are functional:
σ[c.φ := delib] is σ with that one field replaced. Premises are listed in the
order the reducer checks them; when a premise fails, ⇑ yields the reject named
by the first failure, which makes the reject relation a function.

### Roster

[seen] is total. A newcomer enters unseated at weight 0; a returnee keeps
their seat.

```
──────────────────────────────────────────────────────────  [seen]
ρ, σ ⊢ seen(t, p, name) ⇓ ρ[p ↦ ⟨name, ρ(p).s?, ρ(p).w?, t⟩], σ
                            (s? = ⊥, w = 0 when p ∉ dom(ρ))
```

[sync] is total and wholesale: every seat is vacated, then the members are
installed. A principal absent from the sync keeps their identity and loses the
bench.

```
ρ₀ = { p ↦ ρ(p)[s := ⊥, w := 0] | p ∈ dom(ρ) }
─────────────────────────────────────────────────────────────  [sync]
ρ, σ ⊢ sync(t, m*) ⇓ ρ₀[ pᵢ ↦ ⟨nameᵢ, sᵢ, wᵢ, t⟩ | mᵢ ∈ m* ], σ
```

### Opening

```
sovereign(ρ, p)         c ∉ dom(C)          brief ≠ ε
h = required ⇒ subj ≠ ⊥
k = appeal   ⇒ tgt = c′ ∧ c′ ∈ dom(C) ∧ C(c′).φ = closed
ρ′ = ρ[subj ↦ unseated]  if subj ∉ dom(ρ), else ρ
──────────────────────────────────────────────────────────  [open]
ρ, σ ⊢ open(t, c, k, h, p, brief, subj?, tgt?)
     ⇓ ρ′, σ[c ↦ ⟨k, h, intake, p, t, brief, subj?, tgt?, ∅, ⊥, …⟩]
```

Note the environment extension: naming an unknown subject creates them,
unseated. The court can open a case about anyone; it cannot make them a judge.

### Recusal

```
sovereign(ρ, p)      q ∈ dom(ρ)      ¬clerk(ρ, q)
C(c).φ ∈ {intake, noticed}           q ≠ C(c).subj
──────────────────────────────────────────────────  [recuse]
ρ, σ ⊢ recuse(t, c, q, p) ⇓ ρ, σ[c.R := R ∪ {q}]

sovereign(ρ, p)      C(c).φ ∈ {intake, noticed}
──────────────────────────────────────────────────  [unrecuse]
ρ, σ ⊢ unrecuse(t, c, q, p) ⇓ ρ, σ[c.R := R \ {q}]
```

Recusal is only meaningful before the bench freezes; [deliberate] reads R once
and never again.

### The record

Evidence takes the weakest actor premise in the machine — any known principal,
clerk and subject included. Facts are cheap; authority is not.

```
p ∈ dom(ρ)      label ≠ ε      body ≠ ε
C(c).φ ∈ {intake, noticed, delib}       e ∉ dom(Ev)
────────────────────────────────────────────────────────────  [evidence]
ρ, σ ⊢ evidence(t, c, p, e, label, body)
     ⇓ ρ, σ[c.Ev := Ev[e ↦ ⟨t, p, label, body⟩]]

sovereign(ρ, p)      body ≠ ε
C(c).φ ∈ {intake, noticed}      o ∉ dom(O)
────────────────────────────────────────────────────────────  [propose]
ρ, σ ⊢ propose(t, c, p, o, body, π?)
     ⇓ ρ, σ[c.O := O[o ↦ ⟨p, body, π?⟩]]

p ∈ dom(ρ)      C(c).φ ∈ {intake, noticed, delib}
────────────────────────────────────────────────────────────  [cite]
ρ, σ ⊢ cite(t, c, p, π)
     ⇓ ρ, σ[c.cites := cites ∪ {π}]
            [π.cited_in := cited_in ∪ {c}]      (when π ∈ dom(P))

clerk(ρ, p)      body ≠ ε
C(c).φ ∈ {intake, noticed, delib}      n ∉ dom(N)
────────────────────────────────────────────────────────────  [note]
ρ, σ ⊢ note(t, c, p, n, nk, body, cites)
     ⇓ ρ, σ[c.N := N[n ↦ ⟨t, p, nk, body, cites⟩]]
```

[note] is the clerk's only rule, and the only rule that *requires* a clerk:
`HumanCannotClerk` is the dual of `ClerkCannotSovereign`. The seats partition
the acts.

### The hearing

```
sovereign(ρ, p)      C(c).h = required
C(c).φ = intake      C(c).subj ≠ ⊥
──────────────────────────────────────────────────────────  [notify]
ρ, σ ⊢ notify(t, c, p) ⇓ ρ, σ[c.φ := noticed, c.t_notice := t]

C(c).h = required      body ≠ ε
C(c).φ ∈ {noticed, delib}      p = C(c).subj
──────────────────────────────────────────────────────────  [respond]
ρ, σ ⊢ respond(t, c, p, body) ⇓ ρ, σ[c.resp := ⟨t, p, body⟩]
```

Only the subject may respond, and a response may still land during
deliberation — being heard late beats not being heard.

### Freezing the bench

[deliberate] has two shapes, one per hearing mode. This is where ρ is
sampled: the bench is a *snapshot* of the franchise, and no rule after this
one reads ρ for authority over this case.

```
sovereign(ρ, p)      C(c).h = none      C(c).φ = intake
b = bench(ρ, C(c), t)      b ≠ ∅
──────────────────────────────────────────────────────────  [deliberate-record]
ρ, σ ⊢ deliberate(t, c, p) ⇓ ρ, σ[c.φ := delib, c.B := b]

sovereign(ρ, p)      C(c).h = required      C(c).φ = noticed
C(c).resp ≠ ⊥  ∨  t ≥ C(c).t_notice + window(C(c).k)
b = bench(ρ, C(c), t)      b ≠ ∅
──────────────────────────────────────────────────────────  [deliberate-heard]
ρ, σ ⊢ deliberate(t, c, p) ⇓ ρ, σ[c.φ := delib, c.B := b]
```

When the subject is unheard and the window is still open, the reject carries
the deadline: ⇑ SubjectUnheard(t_notice + window(k)).

### Voting

```
p ∈ dom(ρ)      ¬clerk(ρ, p)      reason ≠ ε
C(c).φ = delib      p ≠ C(c).subj      p ∉ C(c).R
⟨p, s, w⟩ ∈ C(c).B      o ∈ dom(O)
──────────────────────────────────────────────────────────  [vote]
ρ, σ ⊢ vote(t, c, p, o, reason)
     ⇓ ρ, σ[c.V := V[p ↦ ⟨t, o, reason, w⟩]]
```

Two things to notice. The weight w comes from the frozen bench B, not from
ρ — losing (or gaining) a Discord role mid-deliberation changes nothing.
And V is a map keyed by voter: a second ballot from p *replaces* the first.
Revoting is legal; ballot-box stuffing is structurally impossible.

### Closing

[close] is the only rule with a nontrivial decision procedure, and it comes
in two shapes that are both *accepted* folds. A failed tally is not a reject;
it is a lapse. The machine refuses acts, never verdicts.

```
⟨p, _, _⟩ ∈ C(c).B  ∨  sovereign(ρ, p)          C(c).φ = delib
k = C(c).k        T = tally(C(c).O, C(c).V)
(m, tied) = margin(T.ordering)    sit = sitting(C(c).B)
w₁ = weight at the head of T.ordering

quorum(T.cast, sit, Q(k))  ∧  ¬tied  ∧  m ≥ M(k)  ∧  w₁ > 0
v = ⟨t, p, winner(T), T.ordering, T.cast, sit, m, C(c).h, C(c).B⟩
──────────────────────────────────────────────────────────  [close-verdict]
ρ, σ ⊢ close(t, c, p) ⇓ ρ, enact(σ[c.φ := closed, c.verdict := v], c, v)

⟨p, _, _⟩ ∈ C(c).B  ∨  sovereign(ρ, p)          C(c).φ = delib
(k, T, m, tied, sit, w₁ as above)
¬( quorum(T.cast, sit, Q(k)) ∧ ¬tied ∧ m ≥ M(k) ∧ w₁ > 0 )
──────────────────────────────────────────────────────────  [close-lapse]
ρ, σ ⊢ close(t, c, p) ⇓ ρ, σ[c.φ := lapsed]
```

The closer premise: membership in the frozen bench suffices, or current
sovereignty as a fallback — a justice who was frozen onto this bench may close
it even after losing the live role. Same principle as [vote]: the bench, once
frozen, is the authority.

The verdict v is a value, not a view: the full ordering (unvoted outcomes at
weight 0), the cast and sitting weights, the margin, and the bench itself are
copied into it. Nothing recomputes later.

`enact` is the only action-at-a-distance in the machine, and it fires only on
a verdict:

```
enact(σ, c, v) =
  σ₁ = if C(c).k = policy ∧ O(v.winner).π = π
       then σ[π.versions := versions · ⟨t, O(v.winner).body, c⟩,
              π.repealed := false]                (creating π if fresh)
       else σ
  if C(c).k = appeal ∧ v.winner = "vacate" ∧ C(c).tgt = c′ ∧ c′ ∈ dom(C)
  then σ₁[c′.φ := vacated]
  else σ₁
```

## Phases, as the machine walks them

```
         notify              deliberate            close, tally holds
intake ─────────▸ noticed ─────────────▸ delib ──────────────────────▸ closed
   │                                      ▴  │
   └──── deliberate (h = none) ───────────┘  └── close, tally fails ─▸ lapsed

closed ── enact, by a later appeal whose verdict is "vacate" ────────▸ vacated
```

closed, lapsed, and vacated are terminal in every rule's premise — no fold
rule fires on them. The single exception is external: a *different* case, of
kind appeal, may move a closed case to vacated through `enact`. History is
immutable; it is only ever reinterpreted by a later court.

## Properties

**Totality.** For every ς = ⟨ev·K, ρ, σ, J⟩ exactly one of [fold], [reject]
applies: ⇓ is defined when all premises of some fold rule hold, ⇑ on the
complement, and the first-failed-premise ordering makes both deterministic.
The machine has no stuck states.

**Determinism.** ↦ is a function, so the final state is determined by K₀
alone. This is the boot theorem: replaying `events.jsonl` from the initial
state reconstructs ρ, σ, and J exactly (`replay_accepted` folds the accepted
subsequence, which by determinism refolds identically).

**Journal completeness.** J is a strictly growing trace, and π₄ (the journal
projection) of a run records every event submitted, with its disposition.
Auditing is reading J; there is nothing else to read.

**Frozen-bench invariant.** After [deliberate] fires on c, no rule consults
ρ to authorize an act *within* c ([vote] and [close] read B). The franchise
can churn; the case cannot feel it.

**Non-sovereignty of clerks.** clerk(ρ, p) disables the premises of [open],
[recuse], [propose], [vote], [deliberate], and the sovereign arm of [close],
and is required by [note]. A clerk's writes are confined to Ev, N, and cites —
the record, never the ruling.
