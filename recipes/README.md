# perch recipes

Ready-made `.perch` files that solve real problems. Download one, audit it with
`perch --scan`, then run it. Each recipe ships unified verbs for a thing you
already use — Redis, Postgres, Docker, kubectl, the whole local-LLM lab —
and replaces the bash one-liners you'd otherwise reach for.

## How to use a recipe

```sh
# 1. Download (a single file — nothing else needed)
curl -fsSL https://raw.githubusercontent.com/luowensheng/perch/main/recipes/redis.perch -o redis.perch
curl -fsSL https://raw.githubusercontent.com/luowensheng/perch/main/recipes/_lib.perch -o _lib.perch

# 2. Audit BEFORE running
perch --scan -f redis.perch

# 3. List what it does
perch -f redis.perch --help

# 4. Run it
perch -f redis.perch up
perch -f redis.perch cli
perch -f redis.perch down
```

Every recipe imports `./_lib.perch` for shared templates (`require_docker`,
`ok`, `say`, etc.) — keep both files in the same directory.

## The catalog

### 🗄️ Single-service stacks

One container, six-to-eight verbs, persistent volumes, idempotent `up`.

| Recipe | What it gives you |
|---|---|
| [`redis.perch`](redis.perch) | Redis 7 + cli + flush + monitor + backup |
| [`postgres.perch`](postgres.perch) | Postgres 16 + psql + createdb + dump + restore |
| [`mongodb.perch`](mongodb.perch) | MongoDB 7 + mongosh + dump + restore |
| [`mysql.perch`](mysql.perch) | MySQL 8 + cli + dump + restore |
| [`mailpit.perch`](mailpit.perch) | SMTP catcher + web inbox — never email yourself from dev again |
| [`minio.perch`](minio.perch) | S3-compatible storage + web console + bucket helper |
| [`rabbitmq.perch`](rabbitmq.perch) | RabbitMQ + management UI |
| [`localstack.perch`](localstack.perch) | AWS emulated locally — S3 / DynamoDB / SQS / SNS / Lambda |

### 🧱 Multi-service stacks (one command, three services)

The real value of perch. Bring up an entire local environment with `parallel`.

| Recipe | What it gives you |
|---|---|
| [`devstack.perch`](devstack.perch) | **Postgres + Redis + MinIO** — the canonical web-app dev stack |
| [`aistack.perch`](aistack.perch) | **Ollama + ChromaDB + Open WebUI** — local LLM lab in one command |
| [`observe.perch`](observe.perch) | **Prometheus + Grafana + Loki** — local observability lab |
| [`kafka-stack.perch`](kafka-stack.perch) | **Kafka (KRaft) + Kafka UI** — broker + web admin, no Zookeeper |

### 🛠️ Cross-platform tool installers

Hide brew / apt / winget differences behind one command.

| Recipe | What it gives you |
|---|---|
| [`modern-unix.perch`](modern-unix.perch) | ripgrep + fd + bat + fzf + jq + yq + eza + zoxide |
| [`clouds.perch`](clouds.perch) | aws + gcloud + az + kubectl + helm + terraform |
| [`node-stack.perch`](node-stack.perch) | fnm + pnpm + project bootstrap + .nvmrc pinning |
| [`python-stack.perch`](python-stack.perch) | uv (fast Python pm) + venv + project bootstrap |

### 🔄 CLI workflow wrappers

Codify the verbs you actually use every day.

| Recipe | What it gives you |
|---|---|
| [`gh-flow.perch`](gh-flow.perch) | `pr / land / sync / cleanup / wip / review / checks` over `gh` |
| [`docker-mgr.perch`](docker-mgr.perch) | `prune-safe / prune-all / big-images / stop-all / top-cpu` |
| [`kube-helpers.perch`](kube-helpers.perch) | `ctx / ns / pods / shell / logs / pf / restart / events` over `kubectl` |

### 🔒 Ops & security

| Recipe | What it gives you |
|---|---|
| [`mkcert-local.perch`](mkcert-local.perch) | Local HTTPS — install root CA, mint per-domain certs |
| [`backup.perch`](backup.perch) | Encrypted backups with `restic` — init / snapshot / restore / forget |
| [`scan-secrets.perch`](scan-secrets.perch) | Scan a repo for committed secrets via `gitleaks` + install a pre-commit hook |

## Why download a recipe instead of writing one?

Every recipe is:

- **One file** — copy, `chmod +x`, run. No clone, no install, no `npm install`.
- **Auditable** — `perch --scan FILE` reports exactly what capabilities it
  needs (shell, network, write paths, env vars) and flags risk findings.
  Read the source; it's never more than ~140 lines.
- **Sandbox-able** — run any recipe with `--no-network` / `--no-write` to
  see what it would do without it actually doing anything destructive.
- **Cross-platform** — the same `commands.perch` works identically on
  macOS / Linux / Windows. No "this is bash-only" footnote.
- **Composable** — recipes import `_lib.perch` for shared templates.
  Your own recipes can do the same — perch's import system makes
  shared helpers a one-liner.
- **MCP-ready** — point `perch-mcp -f redis.perch` at any recipe and an
  AI agent can call its verbs as typed tools. Same file, agent surface
  for free.

## The safety story

Before running a recipe you didn't write, you have three lines of defense:

```sh
# 1. Audit. Reports capabilities, env vars, risk findings.
perch --scan -f recipe.perch

# 2. Dry-run. Print every op with interpolated args, skip execution.
perch --dry-run -f recipe.perch up

# 3. Restrict at run time. Most recipes don't need network or write
#    outside cwd. Layer flags.
perch --no-network --no-write -f recipe.perch status

# 4. Step through with confirmation per op.
perch --ask -f recipe.perch up
```

For the recipes that DO need network/shell (most of them do — they wrap
docker), `--scan` reports exactly which hosts and which binaries.

## Contributing a recipe

Open a PR with a new `recipes/NAME.perch` file. The bar:

- Imports `./_lib.perch` for the shared templates.
- Has a `name`, `about`, `version` block at the top.
- Every command has a `description`.
- `up` is idempotent (safe to run twice).
- Includes `status` and `down` verbs.
- Passes `perch --check -f recipes/NAME.perch`.
- Cross-platform if possible; clearly OS-gated otherwise (`if os == "darwin"`).
- Ships at least one `test`-marked command for `perch test`.

## What this is NOT

These are **starting points**, not turnkey production stacks. The defaults
are dev-friendly — open ports, default passwords, no TLS. Read the source
before deploying anything based on these to a real server.
