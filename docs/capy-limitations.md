# capy grammar limitations — reported, and now resolved

> **Status: all resolved upstream.** This doc originally catalogued nine
> places where perch's surface syntax was shaped by limits in the
> [capy](https://github.com/luowensheng/capy) grammar engine rather than by
> design. They were reported upstream; the engine shipped fixes for **all
> nine** (commits `e1fba0b` + `5102dec`). perch now pins
> `github.com/luowensheng/capy v0.20.1-0.20260529061856-5102decfe5d0` and has
> adopted the ones that improve the language. This page is kept as the record
> of what was wrong, how it was fixed, and what perch took up.

| # | Limitation | Fix in capy | Adopted by perch |
|---|---|---|---|
| 1 | No backtracking after a block-opener commits | automatic backtracking | ✅ (enables flat/block keyword sharing) |
| 2 | Nondeterministic candidate ordering (map iteration) | total order, name tiebreaker | ✅ (no more flaky parses) |
| 3 | No JSON-safe interpolation for ident-or-string | `${asString x}` | ✅ (`exec` argv) |
| 4 | Can't lex flags/paths/globs as one token | `word` capture (+ `tail`) | ✅ (`exec` bare flags) |
| 5 | No context-sensitivity / lookahead | `when_followed_by` / `when_not_followed_by indent` | ✅ (bare `os`/`arch` in `requires`) |
| 6 | No varargs / overload-ladder boilerplate | `tail` capture | partial (kept a `word` ladder — see below) |
| 7 | `#` line comments don't parse | `comments { line "#" }` | ✅ |
| 8 | `try`/`rescue`/`finally` don't parse | `block_sections` | roadmap (grammar exists; interpreter wiring pending) |
| 9 | Dotted access not captured bare | `dotted_ident` | roadmap (`match "${err.kind}"` still the shipped form) |

## What perch adopted, concretely

- **`#` comments (§7).** `lib.capy` now declares `comments { line "#" }`. Leading and trailing `#` comments parse and are ignored — examples and user files can use them freely.

- **`exec` with bare flags and spaced args (§3 + §4).** The `exec` grammar uses `word` captures + `${asString}`, so a token can be a bare flag/path/glob *or* a quoted string with embedded spaces, each landing in exactly one argv slot:

  ```perch
  exec git log --oneline -10              # bare flags — no quotes
  exec docker run -d --name web nginx     # bare paths/names
  exec git commit -m "fix the bug"        # quoted token kept as ONE slot
  ```

  This replaced the previous quote-everything ladder (`exec docker "run" "-d"`).

- **Deterministic flat/block keyword sharing (§1 + §2 + §5).** The `requires` block's `os "linux"` / `arch "amd64"` allowlist entries now share the bare `os`/`arch` keyword with the `os "…" … end` / `arch "…" … end` conditional blocks, disambiguated by `when_not_followed_by indent` (flat entry) vs `when_followed_by indent` (block). This **undid the earlier `run_on` / `run_arch` rename** that the collision had forced.

## Notes on the two not-yet-adopted

- **§6 `tail` (unbounded varargs).** `tail` removes the arity cap, but it **strips quotes** when rejoining tokens, so `exec git commit -m "fix the bug"` would collapse to `commit -m fix the bug` — losing the slot boundary for spaced args. perch therefore kept a `word`-ladder (capped at bin + 8 args), which is lossless for both bare flags and spaced args. If a future capy `tail` preserves quoting (or yields a token array), the ladder can collapse to one function.

- **§8 `try`/`rescue`/`finally` and §9 `dotted_ident`.** The capy grammar primitives now exist (`block_sections`, `dotted_ident`). Wiring them through perch's loader + interpreter (a `try` block op with `rescue`/`finally` sub-bodies; a bare `match err.kind`) is tracked but not yet shipped — `match "${err.kind}"` remains the working error-discrimination form today.
