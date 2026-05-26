# What perch is for

This is the long-form answer to *"why would I use this?"* Each section below is a real category of work that perch is good at, with a concrete sketch and an honest take on why it beats the alternatives. There's also a section at the end on **when not** to use perch.

> If you only have 30 seconds: perch is good wherever you'd otherwise hand-roll a CLI with subcommands, a Makefile, a bash script, or a YAML-driven runner — *and* you want it to work on macOS / Linux / Windows out of the box, *and* you'd like to ship the result as a single binary, web UI, or MCP server without writing extra code.

---

## The two leverage points that recur

Before the application catalog, two perch capabilities show up in almost every use case. Worth holding in your head.

**1. One file → four frontends.** A single `commands.perch` is callable as:

- a CLI (`perch <cmd>` or your-named-binary after `--build`)
- a web UI (`perch --server`)
- a REPL (`perch --shell`)
- an MCP tool surface for AI agents (`perch-mcp`)

You write the file once. Which frontend matters depends on who's using it: developers want the CLI, operators want the web UI, AI agents want MCP. Same source.

**2. `perch --build` produces a self-contained binary.** The output is a fat binary with your program JSON appended; no Go toolchain or perch install needed on the recipient's machine. This collapses the "make a CLI tool" problem from "write a Cobra app, set up packaging, write a release pipeline" down to a single command.

Most applications below lean on one or both of these.

---

## Application catalog

### 1. Replace your Makefile

The base case. Every `make target:` becomes a `command NAME ... end`. Cross-cutting variables go in `globals`. The `if os == "darwin"` form covers the platform-conditional code you'd normally write with three Makefiles.

```capy
globals
    BIN_DIR  = "./bin"
    APP_NAME = "myapp"
end

command build
    arg target
        type string
        default "darwin"
        description "GOOS to build for"
    end
    do
        mkdir "${BIN_DIR}/${target}"
        shell "GOOS=${target} go build -o ${BIN_DIR}/${target}/${APP_NAME} ./cmd/${APP_NAME}"
    end
end

command test
    description "Run tests"
    do
        shell "go test -race ./..."
    end
end

command ci
    description "Lint + test + release"
    do
        run test
        shell "go vet ./..."
    end
end
```

**Why perch wins:** Make doesn't work on Windows. `make ci` requires you to call `go vet` with the right quoting in three different OSes. perch's ops are first-class and cross-platform. `--help` is auto-generated and your `commands.perch` is also a CI script — `run: perch ci` in `.github/workflows/ci.yml` is the whole job definition.

→ Worked example: [demos/03-go-project](https://github.com/luowensheng/perch/tree/main/demos/03-go-project) and [tutorials/01-replace-your-makefile](tutorials/01-replace-your-makefile.md).

---

### 2. Ship an internal team CLI as one binary

Your team has a folder of bash scripts: `bin/backup`, `bin/restart-api`, `bin/check-disk`. Half the team has the folder cloned, half doesn't. Onboarding takes 20 minutes of "git clone, add to PATH, sudo this, run that."

Fold the whole thing into one `commands.perch`, then:

```sh
perch --build -o ops
scp ops bastion:/usr/local/bin/
```

The recipient runs `./ops backup` immediately. No git clone. No Python venv. No node_modules.

```capy
name    "ops"
about   "Platform team operations CLI"
version "1.0.0"

command backup
    description "Snapshot the primary DB to S3"
    do
        let stamp = now "unix"
        shell "pg_dump -h db-primary.internal -Fc > /tmp/db-${stamp}.dump"
        shell "aws s3 cp /tmp/db-${stamp}.dump s3://backups/db-${stamp}.dump"
        rm "/tmp/db-${stamp}.dump"
    end
end

command restart_api
    description "Roll-restart the api deployment"
    do
        shell "kubectl rollout restart deployment/api -n prod"
        shell "kubectl rollout status deployment/api -n prod"
    end
end

catch unknown
    do
        print "ops doesn't know '${unknown}'."
        print "Try: ops backup | ops restart_api"
        exit 1
    end
end
```

**Why perch wins:** every alternative requires more steps. Cobra requires you to write argument parsing, help text, and a release pipeline. A bash distribution requires the recipient to install at least bash + the same set of dependencies you used. Click/Python requires Python on the host.

→ Worked example: [tutorials/02-ship-a-tool](tutorials/02-ship-a-tool.md).

---

### 3. Wrap a clunky CLI as a friendly tool

Probably the highest-leverage application — and the one most teams have a use for.

Many tools have a **good engine, bad UX**. Docker is the canonical example: enormously powerful, but the day-to-day flow ("pull the image, run with these mounts and ports, stream logs, stop, clean up") requires memorising six different invocations. Same story for ffmpeg, kubectl, AWS CLI, openssl, rsync, gh, tar, git — every team has a handful of "I always have to look up the flags" tools.

perch lets you wrap any of these with a sane verb-driven CLI in 30 lines. The shippable binary doesn't require the recipient to learn the underlying tool at all.

Concrete example — a friendly Redis-via-Docker wrapper:

```capy
name    "redis"
about   "Friendly wrapper around the official Redis Docker image"
version "0.1.0"

globals
    IMAGE     = "redis:7-alpine"
    CONTAINER = "my-redis"
end

command install
    description "Pull the Redis Docker image"
    do
        shell "docker pull ${IMAGE}"
    end
end

command run
    description "Start Redis in the background on the host port"
    arg port
        type int
        default 6379
        description "Host port to expose Redis on"
    end
    do
        shell "docker run -d --rm --name ${CONTAINER} -p ${port}:6379 ${IMAGE}"
        print "Redis running on port ${port}. Use 'redis cli' to connect."
    end
end

command cli
    description "Open a redis-cli session into the running container"
    do
        shell "docker exec -it ${CONTAINER} redis-cli"
    end
end

command logs
    description "Stream container logs"
    do
        shell "docker logs -f ${CONTAINER}"
    end
end

command status
    description "Is Redis running?"
    do
        shell "docker ps --filter name=${CONTAINER}"
    end
end

command stop
    description "Stop the running container"
    do
        shell "docker stop ${CONTAINER}"
    end
end

command uninstall
    description "Stop and remove the image"
    do
        run stop
        shell "docker rmi ${IMAGE}"
    end
end
```

Then:

```sh
perch --build -o redis
./redis install
./redis run -port=6380
./redis cli
./redis logs
./redis stop
./redis uninstall
```

The recipient never types `docker` once. `./redis --help` is the discoverable command list. `./redis run --help` shows the typed arg with its default.

**Pair it with `--server`** and someone non-technical clicks buttons:

```sh
./redis --server --port 8080
```

A designer who wants Redis locally for testing now opens a browser, clicks "run", and never thinks about Docker. The wrapper is the user-facing interface; the underlying tool is an implementation detail.

**Pair it with MCP** and an AI agent uses the same surface:

```jsonc
{
  "mcpServers": {
    "redis": { "command": "perch-mcp", "args": ["-f", "/abs/redis.perch"] }
  }
}
```

The agent sees `redis_install` / `redis_run` / `redis_cli` / `redis_stop` — semantic verbs, not Docker syntax. It can reliably orchestrate complex sequences without learning Docker.

**Other "good engine, bad UX" candidates** where this pattern shines:

| Underlying tool | What you wrap | Example commands |
|---|---|---|
| **Docker** / Podman | Image lifecycle for a specific service | `install` / `run` / `cli` / `logs` / `stop` / `uninstall` |
| **kubectl** | Your team's deploy / rollback / status flows | `deploy` / `rollback` / `status` / `logs` / `exec` |
| **ffmpeg** | Specific recipes you actually use | `to_mp4` / `extract_audio` / `make_gif` / `compress` / `concat` |
| **AWS CLI** | Operations your team does | `list_instances` / `rotate_key` / `fetch_logs` / `tail_cloudwatch` |
| **gh** (GitHub) | Team-specific workflows | `start_feature` / `open_pr` / `rebase_main` / `release_notes` |
| **openssl** | Cert lifecycle | `generate_cert` / `verify_cert` / `convert_pem` / `inspect` |
| **rsync** | Curated backup / sync recipes | `backup_docs` / `sync_dotfiles` / `mirror` |
| **tar / unzip** | Project-specific archive flows | `pack_release` / `unpack_release` |
| **git** | Internal workflow conventions | `start_feature` / `ship` / `fixup` / `cleanup_branches` |
| **terraform** | Plan/apply/destroy with team guardrails | `plan_staging` / `apply_staging` / `destroy_staging` |
| **psql / pg_dump** | DB ops | `db_backup` / `db_restore` / `db_psql` / `db_migrate` |
| **gpg** | Encrypt/decrypt with sane defaults | `encrypt_for_team` / `decrypt` / `verify_signature` |
| **systemctl / launchctl** | Service management for your services | `start` / `stop` / `restart` / `tail_logs` |

Every one of these is something a team has a "wiki page of incantations" for. Each wiki page → a `commands.perch` → a shippable binary that turns the wiki page into a discoverable, type-safe CLI.

**Why perch wins for this case specifically:**

1. **The wrapper is 30 lines, not a 500-line Cobra app.** No flag-parsing scaffolding, no `--help` boilerplate.
2. **It ships as one binary.** The recipient doesn't install perch, doesn't need Go, doesn't need the recipe file.
3. **Three frontends from one source.** CLI for developers, web UI for non-engineers, MCP for AI agents. You write the recipe once.
4. **`--check` keeps the wrapper honest.** If you typo a flag in `docker run -i …`, `perch --check` won't catch the typo (it's inside a shell string), but typo'd command refs, missing args, broken `run` targets all surface before users hit them.
5. **`--help` is auto-generated** from the `description` fields, with typed args and defaults. No "documentation drift."

This is the application that **most teams will discover first**. Wrapping a known-clunky tool is the gateway use case; from there, the same team finds they can do all the other applications below.

---

### 4. Extend an existing tool with team conventions (catch passthrough)

A close cousin of section 3, but a distinct pattern with its own use case. Instead of *replacing* a tool's UX, you **extend** it: add your team's high-value shortcuts on top while still letting users (and muscle memory) reach the underlying tool for anything else.

Inside `catch`, perch binds the full unknown invocation as `${proxy_args}`. Forward it to the real binary and you have a drop-in superset.

```capy
name "git"

command ship
    description "Commit all changes, push HEAD, open a PR"
    arg msg
        type string
        description "Commit message"
    end
    do
        shell "git add -A"
        shell "git commit -m '${msg}'"
        shell "git push -u origin HEAD"
        shell "gh pr create --fill"
    end
end

command fixup
    description "Amend the most recent commit with current changes"
    do
        shell "git add -A"
        shell "git commit --amend --no-edit"
    end
end

command cleanup
    description "Delete branches already merged into main"
    do
        shell "git branch --merged main | grep -v '^[* ]*main$' | xargs -r git branch -d"
    end
end

catch passthrough
    description "Forward unknown commands to real git"
    do
        shell "git ${proxy_args}"
    end
end
```

Then `perch --build -o git-team`:

| Invocation | What happens |
|---|---|
| `git-team ship -msg="fix bug"` | Your custom multi-step workflow |
| `git-team fixup` | Amend + no-edit |
| `git-team cleanup` | Prune merged branches |
| `git-team status` | Falls through to real `git status` |
| `git-team log --oneline -10` | Falls through; args preserved |
| `git-team rebase -i HEAD~3` | Falls through |

**Why perch wins:** you get a *superset* of the underlying tool, not a replacement. Users keep their muscle memory; your team's conventions become first-class. `--help` lists only the additions, so people discover the new commands; familiar commands still work without docs.

Other tools that benefit from extension over replacement:

- **`kubectl`** — add `kc deploy <svc>`, `kc rollback <svc>`, `kc tail <svc>`; passthrough for `kc get pods`, `kc describe`, etc.
- **`terraform`** — add `tf plan-staging`, `tf apply-staging`, `tf destroy-staging` with safety guards; passthrough for `tf init`, `tf fmt`.
- **`docker`** — add `d up`, `d down`, `d shell`; passthrough for `d ps`, `d inspect`.
- **`npm`** — add `n release`, `n bump-major`, `n publish-canary`; passthrough for `n install`, `n run`.

---

### 5. LLM-safe wrapper around dangerous tools

When you give an AI agent access to a powerful tool, the grammar perch enforces becomes the security boundary. Instead of letting Claude / Cursor / Zed call `kubectl` or `aws` or `terraform` directly via a shell tool — where any prompt injection could cause arbitrary damage — you expose only the operations you've vetted, with explicit confirmation tokens for destructive ones.

```capy
name "prod-kubectl"
about "AI-safe surface for production kubectl ops"

# ── Read-only ops — agents can call these freely ───────────────────

command get_pods
    description "List pods in prod"
    do
        shell "kubectl get pods -n prod"
    end
end

command logs
    description "Tail recent logs of one pod"
    arg pod
        type string
        description "Pod name"
    end
    arg lines
        type int
        default 100
        description "How many lines"
    end
    do
        shell "kubectl logs -n prod ${pod} --tail=${lines}"
    end
end

command status
    description "Cluster snapshot"
    do
        shell "kubectl get all -n prod"
    end
end

# ── Mutating ops — require an explicit confirmation token ─────────

command restart_api
    description "Roll-restart the api deployment (mutating)"
    arg confirm
        type string
        description "Must be 'YES' — proves the agent meant to mutate"
    end
    do
        if confirm != "YES"
            fail "restart_api requires -confirm=YES (got: '${confirm}')"
        end
        print "Restarting api in prod…"
        shell "kubectl rollout restart deployment/api -n prod"
        shell "kubectl rollout status deployment/api -n prod"
    end
end

# ── No catch-all — anything else is rejected hard ─────────────────
```

Wire it into the MCP client:

```jsonc
{
  "mcpServers": {
    "prod-k8s": {
      "command": "perch-mcp",
      "args": ["-f", "/abs/prod-kubectl.perch"]
    }
  }
}
```

The agent sees `get_pods`, `logs`, `status`, `restart_api`. It **cannot** call `kubectl delete pod`, `kubectl drain node`, `kubectl scale --replicas=0`. The grammar is exhaustive: every callable operation is declared.

**Multi-layer safety the wrapper enforces:**

1. **Whitelist by declaration.** Unlisted operations are unreachable. No catch-all, no shell escape.
2. **Typed args.** `arg lines int default 100` — the agent can't sneak in a `; rm -rf /` (the value goes into a structured op, not a shell string).
3. **Confirmation tokens.** Mutating ops require `-confirm=YES` (or "TYPED_OUT_REASON" for stricter cases). The agent must produce the literal string, which means the LLM has to "intend" the mutation.
4. **Audit log for free.** `perch --server` streams every op as NDJSON. Pipe to your observability stack and you have a complete audit trail of every agent action.
5. **`--check` keeps the policy honest.** As you add ops, the validator catches typo'd arg references / unresolved placeholders / unreachable branches before they ship.

Compare to giving the agent raw `kubectl`: prompt injection or hallucination → cluster damage. With perch's curated surface, the worst the agent can do is something you decided was safe to expose.

**Same pattern for other "dangerous engine" tools:**

- **`rm`** / file-system ops — only the deletes you've vetted; everything else absent
- **`terraform apply`** — only against pre-named environments; only after `terraform plan` (encoded as a `run plan_first` op)
- **`aws`** / cloud APIs — only the read/write surfaces your security team approves
- **DB clients** — only the queries you've parameterised; no raw SQL

> The grammar is the policy. The schema **is** the boundary.

---

### 6. Unify multiple binaries under one tool

Your dev environment depends on 10 tools: node, postgres, redis, kubectl, helm, terraform, jq, ripgrep, gh, docker. New hires spend half a day installing them, often wrong, with version drift. Status checks ("which version of helm do I have?") are a folder full of one-liners.

A single perch binary becomes your team's *meta-tool*:

```capy
name "devtools"
about "Install / status / update the team's dev toolkit"
version "0.1.0"

globals
    NODE_VER  = "20"
    PG_VER    = "15"
    HELM_VER  = "3.14"
    TF_VER    = "1.7"
end

command install_all
    description "Install everything"
    do
        run install_node
        run install_postgres
        run install_redis
        run install_kubectl
        run install_helm
        run install_terraform
        run install_jq
        run install_ripgrep
        run install_gh
        run install_docker
        print "All tools installed."
    end
end

command install_node
    do
        if exists "/usr/local/bin/node"
            print "node already installed"
        end
        if not exists "/usr/local/bin/node"
            if os == "darwin"
                shell "brew install node@${NODE_VER}"
            end
            if os == "linux"
                shell "curl -fsSL https://deb.nodesource.com/setup_${NODE_VER}.x | sudo bash -"
                shell "sudo apt-get install -y nodejs"
            end
        end
    end
end

command install_postgres
    do
        if os == "darwin"
            shell "brew install postgresql@${PG_VER}"
        end
        if os == "linux"
            shell "sudo apt-get install -y postgresql-${PG_VER}"
        end
    end
end

# … one install command per tool …

command status
    description "What's installed, what's missing"
    do
        let n  = exists "/usr/local/bin/node"
        let pg = exists "/usr/local/bin/psql"
        let r  = exists "/usr/local/bin/redis-cli"
        let k  = exists "/usr/local/bin/kubectl"
        let h  = exists "/usr/local/bin/helm"
        let tf = exists "/usr/local/bin/terraform"
        print "node:        ${n}"
        print "postgres:    ${pg}"
        print "redis:       ${r}"
        print "kubectl:     ${k}"
        print "helm:        ${h}"
        print "terraform:   ${tf}"
    end
end

command update_all
    description "Re-run installers; package managers handle the upgrade"
    do
        run install_all
    end
end

command uninstall_all
    description "Remove everything (irreversible — asks for confirm)"
    arg confirm
        type string
        description "Must be YES"
    end
    do
        if confirm != "YES"
            fail "Pass -confirm=YES to actually uninstall"
        end
        if os == "darwin"
            shell "brew uninstall node@${NODE_VER} postgresql@${PG_VER} redis kubectl helm terraform jq ripgrep gh"
        end
        if os == "linux"
            shell "sudo apt-get remove -y nodejs postgresql-${PG_VER} redis kubectl jq ripgrep gh"
        end
    end
end
```

`perch --build -o devtools` and now your team's dev-environment setup, status, update, and teardown all live behind one binary:

```sh
./devtools install_all       # one-time onboarding
./devtools status            # which tools are present?
./devtools install_helm      # add one missing tool
./devtools update_all        # bump everything
./devtools uninstall_all -confirm=YES   # before reimaging the machine
```

**Why perch wins:** the alternative is an `install.sh` per tool, a `README#installation` section that lies, and a Slack channel where people ask "what version of helm do you have?" The unified binary collapses all of that into one auditable source.

Add `--server` and your laptop-provisioning team gets a web UI. Add MCP and the AI agent helping a new hire can install missing tools on demand.

**Variations of "multi-binary unifier":**

- **Polyglot language manager** — wraps asdf / mise / pyenv / nvm / rustup under one set of commands (`langs install node 20`, `langs use python 3.11`).
- **Cloud provider unifier** — same verb-set across AWS/GCP/Azure (`cloud list-instances`, `cloud start-vm`).
- **Service stack starter** — `stack up` brings up postgres + redis + the app, with `if exists` guards for already-running services.
- **Backup destination router** — `backup s3`, `backup b2`, `backup rsync` — same recipe, different targets.
- **Editor/IDE plugin manager** — install / update / list plugins for vim, vscode, neovim from one CLI.

---

### 7. Cross-platform machine setup / new-hire onboarding

A new engineer joins. They need to install ~10 packages, clone two private repos, set five env vars, and run `make init`. Today that's a README with instructions that drift, or a `setup.sh` that doesn't work on Windows.

```capy
command setup
    description "Bootstrap a fresh dev machine"
    do
        if os == "darwin"
            shell "brew install jq ripgrep watchexec gh"
        end
        if os == "linux"
            shell "sudo apt-get install -y jq ripgrep gh"
        end
        if os == "windows"
            shell "choco install jq ripgrep watchexec gh -y"
        end
        if not exists "${HOME}/.team-config"
            shell "git clone git@github.com:org/team-config ${HOME}/.team-config"
        end
        write_file "${HOME}/.zshrc.team" "export TEAM_CONFIG=${HOME}/.team-config\n"
        print "Done. Add 'source ~/.zshrc.team' to your shell rc."
    end
end
```

Then `perch --build -o team-bootstrap` and your onboarding doc shrinks to:

```sh
curl -fsSL https://internal/team-bootstrap -o bootstrap && chmod +x bootstrap && ./bootstrap setup
```

**Why perch wins:** one file covers three OSes with first-class branching. `if not exists` is the conditional you'd otherwise express with `[ -d X ] || ...`. The result ships as a single binary, so the user doesn't need to install Node, Python, or perch first.

→ Worked example: [tutorials/03-cross-platform-installer](tutorials/03-cross-platform-installer.md) and [demos/02-cross-platform-setup](https://github.com/luowensheng/perch/tree/main/demos/02-cross-platform-setup).

---

### 8. Safe operational surface for non-engineers

Your support team needs to flush a customer's cache, regenerate an invoice, or rotate an API key. Today they Slack you and you do it from your laptop. Or worse, you give them shell access.

perch's web UI mode turns `commands.perch` into a real internal tool:

```capy
command flush_cache
    description "Flush the cache for one customer"
    arg customer_id
        type string
        description "Customer UUID"
    end
    do
        shell "redis-cli DEL cache:${customer_id}"
        print "Cache cleared for ${customer_id}."
    end
end

command rotate_key
    description "Rotate the API key for one customer"
    arg customer_id
        type string
        description "Customer UUID"
    end
    do
        let new_key = sha256_file "/dev/urandom"
        shell "psql -c \"UPDATE customers SET api_key='${new_key}' WHERE id='${customer_id}'\""
        print "New key: ${new_key}"
    end
end
```

```sh
perch --server --port 8080 --host 0.0.0.0
```

The support team gets a form for each command, with arg validation. Output streams as NDJSON to the browser. They never touch a shell. You expose exactly the operations you want, no more.

**Why perch wins:** writing this as a Flask/Express app is a multi-day project. perch makes it minutes. The `private` modifier hides helper commands that shouldn't be on the dashboard.

---

### 9. CI pipeline as code (one source for local + remote)

The classic problem: your `.github/workflows/ci.yml` and your local `make test` slowly diverge. Six months in, "it passes locally but fails in CI" is the team's most-said phrase.

```capy
command ci
    description "What CI runs end-to-end"
    do
        run lint
        run test
        run release
    end
end

command lint
    do
        shell "go vet ./..."
        if exists "${HOME}/go/bin/staticcheck"
            shell "${HOME}/go/bin/staticcheck ./..."
        end
    end
end

command test
    do
        shell "go test -race -cover ./..."
    end
end

command release
    do
        shell "GOOS=darwin  go build -o bin/darwin/myapp  ./cmd/myapp"
        shell "GOOS=linux   go build -o bin/linux/myapp   ./cmd/myapp"
        shell "GOOS=windows go build -o bin/windows/myapp.exe ./cmd/myapp"
    end
end
```

The whole CI definition shrinks:

```yaml
# .github/workflows/ci.yml
- run: perch ci
```

**Why perch wins:** the matrix lives in `commands.perch`, not in YAML you also have to maintain. Developers run the exact same `perch ci` locally. Drift becomes impossible.

---

### 10. Data pipeline / ETL orchestration

You have 10 shell scripts that run nightly via cron. They download, decompress, transform, upload. Half of them broke once when curl's flag syntax changed; the other half broke when someone renamed a JSON field.

```capy
command nightly_etl
    description "Pull the daily report, transform, upload"
    do
        let stamp = now "date"
        download "https://reports.example.com/${stamp}.json.gz" "/tmp/r.json.gz"
        ungzip "/tmp/r.json.gz" "/tmp/r.json"
        let body = read_file "/tmp/r.json"
        let users = json_get body "users"
        let count = length users
        if count == 0
            fail "report has no users"
        end
        let payload = json_stringify users
        http_post "https://warehouse.example.com/ingest" payload
        rm "/tmp/r.json"
        rm "/tmp/r.json.gz"
        print "ETL complete: ${count} users."
    end
end
```

**Why perch wins:** `download`, `ungzip`, `json_get`, `http_post` are first-class ops with consistent error handling. No shell quoting drama. `--check` flags typos before the cron fires. If you need this to be more reliable, drop in `if_exists` guards and `fail` checks.

Not a full Airflow replacement, but for the "I have 10 shell scripts and 5 of them have bit-rotted" tier, it's a substantial upgrade.

---

### 11. AI-agent tooling (curated MCP surface)

Give an AI agent (Claude Desktop, Claude Code, Cursor, Zed) the ability to *do things* in your environment — restart services, query metrics, fetch logs — without giving it shell access.

`perch-mcp` exposes the program as two MCP tools (`perch_list`, `perch_run`). Configure it in your MCP client:

```json
{
  "mcpServers": {
    "ops": {
      "command": "perch-mcp",
      "args": ["-f", "/abs/path/to/ops.perch"]
    }
  }
}
```

The agent now sees only the operations you declared. It can call them with structured args. Output streams back.

**Why perch wins:** the grammar is the security boundary. An MCP server that wraps `bash -c` is a footgun; one that wraps a fixed list of typed commands is auditable. Mark helper commands `private` and they're not even visible to the agent.

This is a sweet spot: **curated, safe, structured execution for AI agents.** A small but real industry trend.

→ Setup: [docs/mcp.md](mcp.md).

---

### 12. Self-service runbooks

Every on-call engineer has a folder of runbooks. They're markdown files with shell snippets. Half the snippets stop working when the cluster gets upgraded.

Convert each runbook to a command:

```capy
command runbook_db_failover
    description "Failover the primary DB to the standby"
    do
        print "Step 1: drain connections"
        shell "kubectl exec -n prod db-primary -- pg_drain"
        print "Step 2: promote standby"
        shell "kubectl exec -n prod db-standby -- pg_ctl promote -D /var/lib/postgresql/data"
        print "Step 3: update DNS"
        shell "aws route53 change-resource-record-sets --hosted-zone-id Z123 ..."
        print "Done. Verify with: perch runbook_db_status"
    end
end

command runbook_db_status
    description "Read replication lag from each replica"
    do
        shell "kubectl exec -n prod db-primary -- psql -c 'SELECT * FROM pg_stat_replication'"
    end
end
```

`perch --help` becomes your runbook index. `perch runbook_db_failover` is the runbook itself. CI runs `perch --check` against the file weekly to catch bit-rot before an incident does.

**Why perch wins:** runbooks become executable. Markdown is a lie that the runbook still works; perch's static validator + the fact that the command is the runbook means there's a single source of truth.

---

### 13. Configuration management (Ansible-lite)

For small jobs that don't justify Ansible/Chef/Puppet:

```capy
command provision_box
    description "Bring a fresh VM up to spec"
    do
        if os == "linux"
            shell "sudo apt-get update -qq"
            shell "sudo apt-get install -y nginx postgresql-client"
        end
        write_file "/etc/nginx/sites-available/myapp" "
server {
    listen 80;
    server_name myapp.local;
    location / {
        proxy_pass http://localhost:3000;
    }
}
"
        shell "sudo ln -sf /etc/nginx/sites-available/myapp /etc/nginx/sites-enabled/"
        shell "sudo systemctl reload nginx"
    end
end
```

Run it locally for the first time, then `perch --build -o provision` and SCP to the target. The target machine doesn't need anything pre-installed.

**Why perch wins:** Ansible is great but overkill for a 20-line setup. Bash works but needs `set -euo pipefail` and quoting discipline. perch sits in between: declarative, typed args, op handlers that don't break on whitespace.

---

### 14. Multi-target build orchestration for game / native development

Game studios + native-app teams have weird per-platform build dances: Xcode for iOS, Gradle for Android, MSBuild for Windows, plus signing + notarization + uploads to four stores.

```capy
command release
    description "Cross-platform release"
    do
        run build_ios
        run build_android
        run build_macos
        run build_windows
        run upload_all
    end
end

command build_ios
    require_os "darwin"
    do
        shell "xcodebuild -scheme MyGame -configuration Release archive"
        shell "xcodebuild -exportArchive -archivePath ...  -exportPath ./build/ios"
    end
end

# ...
```

`require_os "darwin"` enforces that iOS builds can only run on a Mac. The rest of the matrix branches via `if os ==`.

**Why perch wins:** the platform branching is structural, not buried in shell `case` statements. CI failures point at the exact step. `--build` lets you ship a `release-cli` binary to designers who need to do trial builds without learning the build system.

---

### 15. Personal "me" CLI (dotfiles, side projects)

Personal automation. The kind of thing where you'd otherwise have `~/bin/blog-deploy`, `~/bin/backup`, `~/bin/cleanup`:

```capy
command deploy_blog
    do
        cd "${HOME}/projects/blog"
        shell "hugo --minify"
        shell "rsync -av public/ user@host:/var/www/blog/"
    end
end

command backup
    do
        let stamp = now "date"
        tar_create "${HOME}/Documents" "/tmp/docs-${stamp}.tar.gz"
        shell "rclone copy /tmp/docs-${stamp}.tar.gz dropbox:Backups/"
        rm "/tmp/docs-${stamp}.tar.gz"
    end
end

command cleanup
    do
        rm "${HOME}/Library/Caches/Google/Chrome"
        rm "${HOME}/Library/Developer/Xcode/DerivedData"
        print "Cleared caches."
    end
end
```

Build it once (`perch --build -o ~/bin/me`) and now `me deploy_blog` is your tool.

**Why perch wins:** you'd otherwise be looking up `tar` flag syntax for the 50th time. perch's ops are stable, autocompleted (via LSP), and `--check`-able.

---

### 16. Scaffold / template generator

You have a "create a new microservice" recipe that does ~20 things: clone a template, rename files, set up CI, register with service discovery, …

```capy
command new_service
    arg name
        type string
        description "Service name (kebab-case)"
    end
    do
        shell "git clone git@github.com:org/service-template ${name}"
        cd "${name}"
        rm ".git"
        shell "git init -q"
        # find/replace placeholders
        shell "find . -type f -exec sed -i '' 's/__NAME__/${name}/g' {} +"
        shell "echo '${name}' > .servicename"
        shell "gh repo create org/${name} --private --source ."
        print "Created ${name}. cd into it and start hacking."
    end
end
```

**Why perch wins:** every team has this recipe. Today it's a shell script with a `set -e` at the top and prayer. With perch, args are typed, descriptions show in `--help`, and `--check` catches typos before you run it on a new repo.

---

### 17. Documentation site / blog tooling

The kind of thing that should be Make but ends up being five separate scripts:

```capy
command preview
    do
        shell_detached "mkdocs serve"
        sleep 2
        if os == "darwin"
            shell "open http://127.0.0.1:8000"
        end
        if os == "linux"
            shell "xdg-open http://127.0.0.1:8000"
        end
    end
end

command publish
    do
        shell "mkdocs build --strict"
        shell "rsync -av site/ user@host:/var/www/docs/"
    end
end
```

**Why perch wins:** the cross-platform `open` / `xdg-open` branching is automatic. `shell_detached` for the background `mkdocs serve` is a real op, not a `&` you have to remember.

---

### 18. Quick API smoke tests / health probes

You want a "is the system OK?" command that pings five services and reports.

```capy
command health
    description "Probe core services and report"
    do
        let api    = http_get "https://api.example.com/health"
        let auth   = http_get "https://auth.example.com/health"
        let db_ok  = port_check "db-primary.internal" "5432"
        let cache  = port_check "cache.internal" "6379"

        print "api:   ${api}"
        print "auth:  ${auth}"
        print "db:    ${db_ok}"
        print "cache: ${cache}"
    end
end
```

Pair with a cron, or expose via `--server` for a real-time dashboard.

**Why perch wins:** `http_get` + `port_check` are first-class. No `curl --fail`-and-grep gymnastics. Combine with `if not api` for failure exits.

---

### 19. AI-assisted refactor / migration tools

Internal tools that combine human-driven and AI-driven operations:

```capy
command migrate_to_v2
    description "Run the v2 migration on this repo"
    do
        shell "go mod edit -droprequire github.com/old/lib"
        shell "go mod edit -require github.com/new/lib@latest"
        shell "find . -name '*.go' -exec sed -i '' 's/old.Foo/new.Foo/g' {} +"
        shell "go mod tidy"
        shell "go vet ./..."
        print "Migration applied. Review the diff and run tests."
    end
end
```

Expose via `perch-mcp` and Claude can drive the migration command-by-command, checking output after each step. The grammar restricts what the agent can do; the migration logic is in your code.

**Why perch wins:** the typed surface is exactly what an LLM needs. Agents handle perch's structured tools far more reliably than `bash -c` with prose.

---

### 20. Embedded scripting for your own Go program

Less obvious but real: you can import perch's capy loader + interpreter as a Go library to give *your* program a scriptable interface. Think: vim's vimscript, or Emacs's elisp, but for a Go application.

```go
import (
    "github.com/luowensheng/perch/infra/capyloader"
    "github.com/luowensheng/perch/infra/interpreter"
    "github.com/luowensheng/perch/infra/ops"
)

func main() {
    prog, err := capyloader.LoadFromString(userScript)
    if err != nil { /* show error */ }
    i := interpreter.New(ops.AllHandlers(), prog)
    i.Run("user_cmd", nil)
}
```

Your users can write `.perch` files that drive your program. You control which op handlers are available — register custom ones for your domain.

**Why perch wins:** if you'd otherwise embed Lua or Starlark, perch gives you a smaller surface, a real LSP for users, and an MCP server for free.

---

### 21. Workshops, tutorials, demos

If you teach DevOps / CI / cross-platform tooling, perch is genuinely a useful classroom tool. The DSL is small enough that students learn it in 20 minutes. The op catalog gives them real capability without "first install jq, brew install …". A `commands.perch` is a small, complete, runnable artifact you can hand them.

**Why perch wins:** alternatives are either too complex (Ansible) or too low-level (bash) to teach in one sitting.

---

### 22. Internal-tools framework for low-engineering teams

Marketing / design / ops / product orgs sometimes want a "button" that triggers something:

- "Recompute weekly metrics"
- "Refresh the staging data from prod"
- "Generate the monthly invoice PDFs"

You write a `commands.perch` with those commands. Run `perch --server --host 0.0.0.0 --port 8080` on a VM. Hand the URL to the team. They click buttons; they fill in args; they see streamed output. No engineer in the loop after setup.

**Why perch wins:** writing this UI in Retool / internal-tool platforms costs $$. perch costs zero. The buttons are command declarations.

---

## When *not* to use perch

This part matters. perch isn't always the right tool.

| Don't reach for perch when… | Use this instead |
|---|---|
| You need full Turing-complete control flow (deep loops, complex data structures, custom types). | A real programming language. perch's `let` + `if EXPR` covers ~80% of scripting needs but isn't the right tool for serious logic. |
| You need a workflow engine with retries, scheduling, observability, backfills. | Airflow, Prefect, Dagster, Temporal. |
| You're doing infrastructure as code at scale (declarative diffs, drift detection). | Terraform, Pulumi, OpenTofu. |
| Multi-host configuration with idempotent diffs. | Ansible, Chef, Puppet. |
| Performance-critical hot paths. | perch's interpreter is fine for a thousand ops; it's not a 10K-ops-per-second engine. |
| You need a real templating / code-generation engine. | capy (perch's underlying DSL engine) or a project-specific generator. |
| Highly dynamic command sets (commands defined at runtime by external data). | Build a real Go program; use perch's interpreter as a library. |

The honest framing: perch wins for **the middle 80% of small tooling tasks**. Below it (one-line `find . -name '*.tmp' -delete` style stuff), bash is fine. Above it (real production pipelines with monitoring + retries), you want a workflow engine. The middle band is where perch is happiest, and that band turns out to be most of the daily-friction tooling work in any organisation.

---

## Combining frontends — compound applications

The four frontends (CLI / server / shell / build / MCP) compose. Some patterns:

**Pattern: "developer tool that ships to ops."**
You build a `commands.perch` you use locally as `perch foo`. Once it's stable, `perch --build -o ops` and hand it to the ops team. They use the same commands; you ship updates by rebuilding.

**Pattern: "AI-assisted ops dashboard."**
Same `commands.perch` runs three ways: ops team uses `perch --server` in a browser; the on-call engineer uses `perch <cmd>` from a terminal; an LLM uses `perch-mcp` to suggest and execute fixes from a chat window. Same commands, three audiences.

**Pattern: "self-replicating dev environment."**
Your team's `dev-setup.perch` does the new-machine setup. Build it (`perch --build -o dev-setup`). Commit the binary to a private bucket. Onboarding doc says `curl … && ./dev-setup setup`. The recipient is now running your perch program — and inside it, they `dev-setup ship-this` to make THEIR project shippable the same way.

**Pattern: "audit log of every op."**
`perch --server` streams every op as NDJSON to clients. Pipe that stream into your observability stack. You get a free audit log of every operation anyone ran against the operational surface — without writing logging into each command.

---

## A few opinionated patterns

Some things that have shaken out as best-practice from real use:

**Put everything stable in `globals`.** Paths, names, build dirs, common env. If you reference it from two commands, hoist it.

**Use `private` aggressively for helpers.** Anything that isn't meant as a top-level operation gets `private` so `--help` stays clean and the MCP surface stays minimal.

**`catch` for human-friendly errors.** If you ship the binary to non-engineers, a `catch unknown` that lists valid commands is the difference between "looks broken" and "looks polished."

**Group commands by verb-first naming.** `db_backup` / `db_restore` / `db_migrate` reads better than `backup_db` / `restore_db` and groups in `--help`.

**Treat `perch --check` as part of CI.** Catch typos, missing `run` targets, unresolved `${name}` placeholders before they hit prod.

**Build for the slowest target first.** A `perch --build -o myapp` on macOS produces a Mach-O arm64 binary. For wider distribution you need to wait for cross-compile support (roadmap), or run `--build` on each target's CI runner.

---

## Where to go next

- Five-minute tour: [getting-started.md](getting-started.md)
- Full grammar: [language.md](language.md)
- Built-in op catalog: [op-reference.md](op-reference.md)
- The fat-binary format: [embedding.md](embedding.md)
- AI integration: [mcp.md](mcp.md)
- Editor support: [lsp.md](lsp.md)
- Demos: [github.com/luowensheng/perch/tree/main/demos](https://github.com/luowensheng/perch/tree/main/demos)

If your use case fits a category above and you've built something interesting, open a PR adding a `demos/` folder for it. The catalog grows from real examples.
