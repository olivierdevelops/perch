# Recipes — ready-made `.perch` files you can download and run

> **22 curated `.perch` files** that solve real problems — local Redis, the
> whole AI/observability/Kafka stack, cross-platform tool installers, daily
> Docker/kubectl wrappers. Download one, audit it with `perch --scan`,
> run it. No clone, no install, no source build.

## TL;DR — one line install + run

```sh
# Download a recipe + its shared library
curl -fsSL https://raw.githubusercontent.com/luowensheng/perch/main/recipes/redis.perch -o redis.perch
curl -fsSL https://raw.githubusercontent.com/luowensheng/perch/main/recipes/_lib.perch -o _lib.perch

# Audit BEFORE running — risk score + what it touches
perch --scan -f redis.perch

# Verify it's feasible on THIS machine without running it
perch -f redis.perch --check

# Run
perch -f redis.perch up
perch -f redis.perch cli
```

Recipes declare what they need in a `requires` block (the Docker recipes
declare `bin "docker"`). With it present, perch runs in **strict mode** —
any undeclared shell bin is refused, and `perch --check` catches it
**statically, before you run**. See [requires.md](requires.md).

## The catalog (22 recipes)

### Single-service local databases / queues (8)

One container, six-to-eight verbs, persistent volume, idempotent `up`.

| Recipe | Verbs | What it gives you |
|---|---|---|
| **`redis`** | `up · down · cli · flush · monitor · logs · backup · status` | Redis 7 with rdb backup |
| **`postgres`** | `up · down · destroy · psql · createdb · dump · restore · logs · status` | Postgres 16 with persistent volume |
| **`mongodb`** | `up · down · destroy · shell · dump · restore · logs · status` | MongoDB 7 with mongosh |
| **`mysql`** | `up · down · destroy · cli · dump · restore · status` | MySQL 8 with persistent volume |
| **`mailpit`** | `up · down · open · logs · status` | SMTP catcher + web inbox |
| **`minio`** | `up · down · destroy · make_bucket · logs · status` | S3-compatible storage + console |
| **`rabbitmq`** | `up · down · destroy · logs · status` | RabbitMQ + management UI |
| **`localstack`** | `up · down · destroy · status · logs` | AWS S3/DynamoDB/SQS/SNS/Lambda emulator |

### Multi-service stacks (4) — perch's real value

Bring up three services with one command, using `parallel`.

| Recipe | Services | Why |
|---|---|---|
| **`devstack`** | Postgres + Redis + MinIO | Canonical web-app local stack — typical dev needs DB + cache + object storage |
| **`aistack`** | Ollama + ChromaDB + Open WebUI | Local LLM lab — model runtime + vector store + chat UI |
| **`observe`** | Prometheus + Grafana + Loki | Local metrics + logs + dashboards |
| **`kafka-stack`** | Kafka (KRaft mode) + Kafka UI | Stream processing dev environment, no Zookeeper |

```sh
$ perch -f devstack.perch up
==> Starting devstack
✓ postgres up
✓ redis up
✓ minio up
✓ devstack ready

  Postgres:  postgres://postgres:postgres@localhost:5432/app
  Redis:     redis://localhost:6379
  MinIO API: http://localhost:9000  (minioadmin/minioadmin)
  MinIO UI:  http://localhost:9001
```

### Cross-platform tool installers (4)

Hide brew / apt / winget differences behind one command.

| Recipe | What it installs |
|---|---|
| **`modern-unix`** | ripgrep + fd + bat + fzf + jq + yq + eza + zoxide |
| **`clouds`** | aws + gcloud + az + kubectl + helm + terraform |
| **`node-stack`** | fnm + pnpm + project bootstrap + `.nvmrc` pinning |
| **`python-stack`** | uv (Astral's fast Python PM) + venv + project bootstrap |

```sh
$ perch -f modern-unix.perch install   # works on macOS / Linux / Windows
$ perch -f modern-unix.perch check
  ✓ rg
  ✓ fd
  ✓ bat
  ✓ fzf
  ✓ jq
  ✓ yq
  ✓ eza
  ✓ zoxide
```

### Daily CLI workflow wrappers (3)

Codify the verbs your team uses, with typed args and per-command `--help`.

| Recipe | Wraps | Verbs |
|---|---|---|
| **`gh-flow`** | `gh` (GitHub CLI) | `pr · land · sync · cleanup · wip · review · checks · status` |
| **`docker-mgr`** | `docker` | `status · big_images · stop_all · prune_safe · prune_all · top_cpu · shell · logs` |
| **`kube-helpers`** | `kubectl` | `ctx · ns · pods · shell · logs · pf · restart · top · events · who_am_i` |

```sh
$ perch -f gh-flow.perch land   # squash-merge current PR + delete branch
$ perch -f docker-mgr.perch prune_safe   # safe prune — no volumes
$ perch -f kube-helpers.perch restart api-deploy
```

### Ops & security (3)

| Recipe | What it does |
|---|---|
| **`mkcert-local`** | Local HTTPS — install root CA, mint per-domain certs |
| **`backup`** | Encrypted backups with `restic` — snapshot/restore/forget/check/mount |
| **`scan-secrets`** | Scan repo + container images for committed secrets via `gitleaks` |

## The safety story — three lines of defense

Before running a recipe you didn't write, you have static checks AND runtime gates:

### 1. `perch --scan` — static audit (no execution)

```
$ perch --scan -f redis.perch
── redis.perch ─────────────────────────────────
8 command(s), no catch, 2 binding(s)

CAPABILITIES NEEDED
  ✓ shell       (12 calls: docker exec, docker run, docker ps, …)
  ✓ subprocess  (the `docker` binary)

ENV VARS REFERENCED
  (none)

RISK FINDINGS
  [LOW]  shell args interpolate ${port} — value is bounded by type=string

RECOMMENDED INVOCATION
  perch --no-network --allow-bin docker --max-runtime 60 -f redis.perch up
```

### 2. `perch --dry-run` — execute nothing, show everything (including block bodies)

```
$ perch --dry-run -f redis.perch up
──── Dry-run — printing plan; no ops execute ────
  [1] has_bin "docker"   → ${has_docker}
  [2] if lhs="has_docker" op="falsy"   {1 body op}
        fail msg="docker is required. Install Docker Desktop, or alias docker→podman."
  [3] shell_output "docker ps -q -f name=^perch-redis$"   → ${running}
  [4] if lhs="running" op="truthy"   {1 body op}
        print msg="✓ ${NAME} already running"
  [5] if lhs="running" op="falsy"   {5 body ops}
        shell_output "docker ps -aq -f name=^${NAME}$"   → ${existing}
        if lhs="existing" op="truthy"   {1 body op}
           shell cmd="docker rm ${NAME}"
        shell cmd="docker run -d --name ${NAME} -p ${port}:6379 ${IMAGE}"
        print msg="✓ redis up on port ${port}"
        print msg="  connect: redis-cli -p ${port}"
```

Block ops (`if`, `parallel`, `retry`, `cache`, `sandbox`, …) have their
bodies expanded inline, so what you see is every op that **could** fire.

### 3. `perch --no-X` flags — runtime restrictions

```sh
perch --no-network -f recipe.perch status      # status doesn't need net
perch --no-write   -f recipe.perch logs        # logs doesn't write
perch --allow-bin docker -f recipe.perch up    # ONLY docker can shell out
```

Combine with `--ask` for per-op confirmation.

## How the recipes are built

Each recipe is **30–140 lines** of straightforward `.perch`. They share a
common library at `recipes/_lib.perch` providing four templates:

- `require_docker` — fail fast if docker is missing
- `require_bin "NAME"` — fail fast if a specific bin is missing
- `say "label"` → `==> label`
- `ok "msg"` → `✓ msg`
- `warn "msg"` → `⚠ msg` (stderr; doesn't fail)

This shows off perch's **import system** (the recipe imports a helper file)
and **template system** (one declaration, used everywhere by name).

## Compose your own

Recipes are starting points. Fork one, change defaults, add verbs, ship it
as your team's `commands.perch`:

```sh
cp recipes/devstack.perch ./my-team-stack.perch
# Edit. Maybe swap MinIO for Elasticsearch. Add `seed` and `reset` verbs.
perch --check -f my-team-stack.perch
perch -f my-team-stack.perch up
```

Or `perch --build -f my-team-stack.perch -o devup` and your colleagues
just run `./devup up`. No perch install needed.

## Status

**Pre-1.0.** Recipes will grow; the import / template / context features
they rely on are stable. Open a PR to add a recipe for the thing you wish
existed.

Source: [github.com/luowensheng/perch/tree/main/recipes](https://github.com/luowensheng/perch/tree/main/recipes).
