# What's missing in capy — the grammar engine's limits

> **Context.** perch's surface syntax is defined entirely in [`infra/capyloader/lib.capy`](../infra/capyloader/lib.capy) and parsed by the [capy](https://github.com/luowensheng/capy) library-function engine. capy is a good fit — a declarative grammar that emits NDJSON events the Go loader folds into a `domain.Program`. But several of perch's syntax compromises trace directly to capy's parser and tokenizer, not to deliberate design. This doc catalogs those gaps, with the concrete symptom, the root cause, where it bites perch, and a suggested fix — so they can be fixed upstream or designed around with eyes open.
>
> Engine version this was written against: `github.com/luowensheng/capy v0.20.1-0.20260527004148-b899b0799afc` (`orchestrator/features/make_parser.go`).

---

## 1. No backtracking after a block-opener commits

**Symptom.** Two functions can share a leading literal where one is a flat statement and the other a block. Once capy matches the block opener's literal + args, it commits to parsing a block body; if no indented body follows, it errors **instead of** trying the flat alternative.

**Root cause.** `parseStmt` (make_parser.go): when a matched function has `fn.Block != nil`, it calls `parseBlockBody`, and on error `return`s the error — there is no `Restore(saved); continue` to fall through to the next candidate function.

```go
body, closer, err := p.parseBlockBody(fn.Block.Closer)
if err != nil {
    return domain.FuncCall{}, err   // ← commits; no backtrack to the flat form
}
```

**Where it bit perch.** The `requires` block's OS/arch entries (`os "linux"`) collided with the `os "…" … end` execution-context conditional block. Both own the bare `os` keyword; whichever capy tried first won, and the loser's context parsed wrong or errored. We had to **rename** the requires entries to `run_on` / `run_arch` to dodge the collision entirely (see CHANGELOG). The same hazard blocks adding `with_exec BIN … end` whose inner bare-argv lines would need context-sensitive parsing.

**Suggested fix.** In `parseStmt`, wrap the block-body attempt in the same `saved := p.Save()` / `p.Restore(saved); continue` pattern already used for the non-block arms, so a failed block parse falls through to the next candidate. This single change would make flat-vs-block keyword sharing safe and unlock `with_exec`.

---

## 2. Nondeterministic candidate ordering (map iteration)

**Symptom.** The *same* source parses successfully on one run and fails on the next (~50% in the worst case), with no input change.

**Root cause.** `MakeParser` builds the candidate slice by ranging a **map** and then sorting it with `sort.SliceStable` keyed on `(Priority desc, startsWithLiteral, literalLength desc)`:

```go
fns := make([]*domain.FuncDef, 0, len(lib.Functions))
for _, f := range lib.Functions { fns = append(fns, f) }   // ← map order: nondeterministic
sort.SliceStable(fns, func(i, j int) bool { /* priority, literal-start, literal-length */ })
```

Functions that **tie on all three sort keys** keep whatever order the map iteration produced — which Go randomizes per range. Combined with #1 (no backtrack), a tie between a flat and a block function for the same keyword flips the parse outcome run-to-run.

**Where it bit perch.** Same `os`/`arch` collision as #1 — the flakiness is what made it visible (a `requires` block with `os "linux"` parsed ~50% of the time). It also means *any* future keyword collision is a latent heisenbug.

**Suggested fix.** Make the sort total: add a final tiebreaker on the function's **name** (or definition order in the source `.capy`). One line in the comparator. This alone would have turned the `os`/`arch` collision from a flaky failure into a deterministic one (still wrong, but debuggable) — and with #1 fixed, into a correct parse.

---

## 3. `any`/`ident` capture interpolation isn't JSON-safe — and `toJSON` over-quotes strings

**Symptom.** There is no single interpolation form that correctly emits *both* a bare identifier and a quoted string as a JSON value.

- `${a}` emits the capture's **source text**. Correct for a quoted string (`"foo"` → already valid JSON), but a bare ident (`foo`) emits `foo` — invalid JSON, "malformed event".
- `${toJSON a}` JSON-encodes the captured **value**. Correct for a bare ident (`foo` → `"foo"`), but a string capture is *already* the string `foo`, so it re-quotes to `"foo"` and then the loader decodes `"foo"` **with the quotes as data** — the value leaks literal `"` characters.

**Root cause.** capy exposes the capture as either source text or a re-encoded value, with no "emit this token as one JSON string, quoting iff it isn't already a string literal" primitive. The `any` type spans string/ident/number/bool, so neither form is right for all of them.

**Where it bit perch.** `exec`'s argv tokens. We wanted the doc's bare form (`exec git status`), but `toJSON` double-quoted real string args (`exec echo "x"` captured `"x"` *with quotes*). The fix was to type argv as `string` and require **every token be quoted** (`exec docker "run" "-d"`), losing the bare-word ergonomics the design sketched.

**Suggested fix.** Add an interpolation verb that normalizes a capture to a single JSON string regardless of whether the lexer saw a quoted string or a bare token — e.g. `${asString a}`. Then `exec BIN tok tok` could accept bare words and quoted strings interchangeably and emit correct JSON for both.

---

## 4. The tokenizer can't lex flags / paths / globs as single tokens

**Symptom.** A bare `--oneline`, `-f`, `k8s/deploy.yaml`, or `name=^web$` doesn't parse as one `any` token; the line fails to match any overload.

**Root cause.** capy's lexer tokenizes on its own rules (identifiers, strings, numbers, punctuation). A leading `-`, an embedded `/`, `=`, `.`, `^`, `$`, or `*` isn't part of an identifier token and there's no "shell word" token class, so these reach the parser as multiple tokens (or not at all).

**Where it bit perch.** The whole point of `exec`/§3.1 was a shell-like bare-argv surface (`exec git log --oneline -10`). It's impossible through the tokenizer — every flag/path/filter has to be a quoted string. (This is the same root cause behind perch command names not allowing hyphens, e.g. `restart-api`.)

**Suggested fix.** A lexer mode or token class for "bare shell word" (a run of non-whitespace, non-quote characters) that a capture could opt into (`arg capture tok word`). This is the single highest-leverage change for making `exec` read like a shell.

---

## 5. No context-sensitivity / negative lookahead

**Symptom.** A function can't say "match `os "X"` **only when** an indented block follows" (or only when one doesn't). The grammar is context-free; a keyword means the same thing everywhere.

**Where it bit perch.** Reinforces #1 — `os` inside `requires` (a flat allowlist entry) vs `os` in a command body (a conditional block) genuinely *should* mean different things by position, but capy can't express it. Forced the `run_on` rename rather than a context rule.

**Suggested fix.** Either lookahead predicates in a function definition (`when_followed_by indent`), or scoped sub-grammars (a set of functions active only inside a given block). The latter also cleanly enables block-local mini-languages (e.g. `requires`-only keywords).

---

## 6. No varargs capture — arity is hand-rolled per overload

**Symptom.** An op that takes "a binary plus N argv tokens" needs one grammar function **per arity**.

**Where it bit perch.** `exec` has `exec_0args … exec_6args` plus a parallel `let_exec_0args … let_exec_4args` set — a dozen near-identical functions, each capped at a fixed count. `run` (`run_1arg … run_4args`) and `call` (`call_0args … call_3args`) have the same shape. Anything past the cap silently doesn't parse.

**Suggested fix.** A repeating capture — `arg capture rest star` — that collects zero-or-more trailing tokens into a JSON array. Collapses a dozen overloads into one function and removes the arbitrary arity cap.

---

## 7. `#` line comments don't parse

**Symptom.** A `#`-prefixed comment line (or trailing `# comment`) inside a `.perch` file is not recognized and breaks the parse.

**Where it bit perch.** Every doc/demo example has to omit comments or risk a parse error; we've repeatedly had to strip inline `# …` from examples (most recently the `requires bin "docker"  # …` line in `applications.md`). Comments are table-stakes for a config/automation language.

**Suggested fix.** Lex `#` (and/or `//`) to end-of-line as a comment token the parser skips. Low effort, high everyday value.

---

## 8. `try` / `rescue` / `finally` don't parse

**Symptom.** The error-handling block form documented for perch doesn't actually parse through the current grammar.

**Where it bit perch.** `docs/errors.md` and `guide.md` carry a "this is parse-broken today" caveat on the `try/rescue/finally` examples. Users reach for it (it's the obvious structured-error form) and hit a wall; `match err.kind` is the working substitute.

**Suggested fix.** This is likely a perch-side `lib.capy` grammar bug interacting with capy's block handling rather than pure-capy — but it needs capy's block parsing (#1, #5) to be solid first. Tracking it here because the diagnosis bottoms out in the same block-parser behavior.

---

## 9. Dotted access isn't an identifier

**Symptom.** `match err.kind` doesn't work bare; the `ident` capture won't consume the `.`.

**Where it bit perch.** Users must write `match "${err.kind}"` (string form) instead of the cleaner `match err.kind`. Documented as a required workaround.

**Suggested fix.** A `dotted_ident` capture type (identifier with `.`-separated segments), or allow `.` inside the `ident` lexer class when between identifier characters.

---

## Priority for perch

If these were fixed upstream, the order that would unlock the most perch value:

1. **#2 (deterministic ordering)** — trivial one-line comparator fix; removes a class of heisenbugs.
2. **#1 (block backtracking)** — unblocks `with_exec` and safe keyword reuse.
3. **#4 (bare-word token) + #3 (JSON-safe interpolation)** — together make `exec`/`pipe` read like a real shell instead of requiring quotes on every token.
4. **#7 (`#` comments)** — small, but every example and user file wants it.
5. **#6 (varargs)** — removes the overload boilerplate and arity caps.
6. **#5, #8, #9** — context-sensitivity, `try/rescue`, dotted idents — larger or dependent on the above.

Everything perch ships today works *around* these; none is a blocker. But each is a place where the surface syntax is shaped by the engine rather than by what would read best.
