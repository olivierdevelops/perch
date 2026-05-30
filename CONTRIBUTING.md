# Contributing to perch

> ## 🚫 perch is NOT accepting external contributions at this time.

The project is currently maintained by one person on a single direction and is
not in a phase where external code contributions, feature requests, or
external maintenance fit the workflow. To set expectations honestly:

| What | Status |
|---|---|
| **Pull requests** | Will be closed unread. Please don't open them. |
| **Feature requests** | Will be redirected to [GitHub Discussions](https://github.com/olivierdevelops/perch/discussions). The maintainer reads Discussions, but ideas land on the roadmap at the maintainer's pace and are implemented by the maintainer. |
| **Bug reports for shipped behavior** | Welcome. Open a [bug issue](https://github.com/olivierdevelops/perch/issues/new?template=bug.yml). Fixes will be authored by the maintainer. |
| **Security issues** | Email the maintainer directly. See [SECURITY.md](SECURITY.md) if/when it exists; otherwise the GitHub profile contact. |
| **Forks** | Encouraged. perch is Apache-2.0 — fork it, ship your own variant, run your own roadmap. |

## Why this stance

perch is at an early-design phase where the surface (DSL grammar, op catalog,
capability model, file format) still moves frequently. Merging external work
during a moving surface creates compatibility commitments the maintainer
isn't ready to make, and quality-controlling drive-by PRs is itself a full-time
job. The simplest honest answer right now is: not accepting.

## What you CAN do

- ⭐ **Star the repo** if you find it useful — that's a real signal.
- 💬 **Open a Discussion** — share what you're building with perch, ask
  questions, propose ideas. The maintainer reads every thread.
- 🐛 **File bugs** — concrete reproduction steps for shipped behavior.
  These get read and acted on.
- 🍴 **Fork it** — Apache-2.0 lets you. Run your own line of development;
  if a published change is useful enough downstream, the maintainer may
  port it (under the maintainer's own pen).
- 📦 **Use it** and tell people. Adoption is the signal that justifies
  more maintainer time.

## When this changes

The stance is "for now" — the project genuinely intends to open up to
external contributions once the core grammar/op-catalog stabilises. When
that happens:

1. This file will be replaced with concrete contribution guidelines
   (style, test expectations, sign-off requirements, etc.).
2. The notice at the top of `LICENSE` will be removed.
3. A note will go out in the release CHANGELOG.

Until then: thanks for understanding, and thanks for using perch.

— [@olivierdevelops](https://github.com/olivierdevelops)
