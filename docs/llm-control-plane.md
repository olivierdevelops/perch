# perch as an LLM control plane

> **You can use perch as a protected execution layer for an LLM, so it only runs the commands you declared — without standing up backend infrastructure.**

That's the strongest framing of what perch is good for: it replaces a custom backend whose only job is letting an agent perform a fixed set of actions safely.

This page walks through the pattern and shows why a `.perch` file + `perch-mcp` + a few CLI flags can do what a 2,000-line FastAPI service used to.

---

## The usual problem

You want an LLM agent to *do* something — restart a service, refund a customer, fetch logs, run a migration, send an email. The conventional shape:

1. Spin up a backend (FastAPI / Express / Go).
2. Define endpoints — one per agent-callable action.
3. Add authentication.
4. Hand-roll input validation per endpoint.
5. Add audit logging middleware.
6. Add rate limiting.
7. Write function-calling glue for whichever LLM framework you use (Claude tool use, OpenAI function calling, LangChain tools).
8. Define a JSON schema per function for the LLM to consume.
9. Keep all of it in sync as the API evolves.
10. Maintain it.

That's a lot of plumbing for "let the agent call these eight functions safely." Most of it is not the agent's actions — it's *the scaffolding to expose actions to an agent at all.*

---

## The perch alternative

Write a `.perch` file. Run `perch-mcp` with whatever restrictions you want. That's it.

```capy
name        "ops"
about       "Operations the agent can perform"
version     "0.1.0"

command restart_pod
    description "Restart a Kubernetes pod"
    arg ns
        type string
        description "Namespace (must match ^[a-z0-9-]+$)"
    end
    arg pod
        type string
        description "Pod name (must match ^[a-z0-9.-]+$)"
    end

    do
        if not regex_match "${ns}" "^[a-z0-9-]+$"
            fail "invalid namespace"
        end
        if not regex_match "${pod}" "^[a-z0-9.-]+$"
            fail "invalid pod name"
        end
        shell "kubectl -n ${ns} delete pod ${pod}"
    end
end

command get_logs
    description "Fetch recent pod logs"
    arg ns    type string end
    arg pod   type string end
    arg lines type int default 100 end

    do
        let logs = shell_output "kubectl -n ${ns} logs ${pod} --tail=${lines}"
        print "${logs}"
    end
end

command scale_deployment
    description "Set the replica count on a deployment"
    arg ns       type string end
    arg name     type string end
    arg replicas type int end

    do
        if replicas > 50
            fail "replicas > 50 needs a human"
        end
        shell "kubectl -n ${ns} scale deploy/${name} --replicas=${replicas}"
    end
end
```

Then:

```sh
perch-mcp --env KUBECONFIG,HOME --no-network -f ops.perch
```

That's the backend.

The agent connects via MCP, calls `perch_list` to discover the verbs, calls `perch_run` with named args. The schema you wrote *is* the API. Anything outside it is rejected with a typed error — no JSON-parsing bugs to hand-write past, no auth bypass to engineer around.

---

## What you get without writing it

| Concern | Custom backend | perch |
|---|---|---|
| Endpoint definition | route + handler code | `command NAME … do … end` |
| Arg schema for the LLM | code-gen or hand-written JSON | auto-exposed via `perch_list` |
| Input validation | hand-rolled per route | typed args (`type string/int/float/bool`), regex_match guards |
| Auth boundary | bespoke middleware | "is the agent allowed to talk to this `perch-mcp` instance" |
| Per-action restrictions | code | declared `command` set; non-declared verbs simply don't exist |
| Filesystem / shell restrictions | not provided by HTTP framework | `--no-shell`, `--no-write`, `--no-subprocess` |
| Network egress restrictions | firewall, separate concern | `--no-network` |
| Env-var visibility | the host's env, all of it | `--env A,B,C` (everything else errors) |
| Audit logging | custom middleware + format | NDJSON stream from `--server`; op-level error trail |
| Error consistency | per-handler | uniform: `op "X" is disabled by --no-Y` / `arg fails regex /…/` / `command not declared` |
| LLM-facing tool schema | maintain by hand | derived from the file — name, description, typed args |
| Auditability of the "API surface" | "read the codebase" | "read the 50-line .perch file" |
| Testing | full backend test infra | `perch --dry-run cmd`, `perch --ask cmd`, `perch --check` |
| Local hand-execution | rare | `perch <cmd>` runs the same path |
| Deploy | container + secrets + ingress | one binary (`go install`); `perch --build` for embedded |

The right column is one file plus a process. The left column is a quarter of a sprint.

---

## The schema IS the controlled-execution boundary

> **Honest scope:** perch is **controlled scripting**, not a kernel-level sandbox. With `--no-shell` the boundary is airtight (no subprocess ever fires). With `shell` allowed, the spawned process can still talk to the kernel — perch only fences its *own* op dispatch. For genuinely adversarial input, layer perch under `firejail` / `sandbox-exec` / `AppContainer`. The rejections below describe the perch-level boundary; the OS-level boundary is your responsibility.

When the agent tries something you didn't declare, there's no defensible-by-default code path it can reach. The chain of rejections at the perch level:

1. **Verb not declared** →

    ```
    command "drop_database" not declared in ops.perch
    ```

2. **Arg type wrong** (agent passes a string where an int is expected) →

    ```
    arg replicas: invalid int "fifty"
    ```

3. **Arg value fails your validation** →

    ```
    invalid namespace
    ```

   (because you wrote `if not regex_match "${ns}" "^[a-z0-9-]+$" fail …`)

4. **Op outside the allowed catalog** (agent crafts an arg that would trigger a banned op — possible because the file uses `shell`; you've also opted into `--no-network`) →

    ```
    op "http_get" is disabled by --no-network (see https://luowensheng.github.io/perch/sandbox/)
    ```

5. **Env var not on the allowlist** (script interpolates `${SECRET_AWS_KEY}` you never declared) →

    ```
    env var ${SECRET_AWS_KEY} is not in --env allowlist (declare with --env SECRET_AWS_KEY)
    ```

6. **HTTP destination is off-allowlist** (agent crafts a URL pointing at a host you never approved) →

    ```
    host "attacker.com" is not in --allow-host allowlist (allowed: api.github.com, *.docker.io)
    ```

    Even a 30x redirect from `api.github.com` to `attacker.com` is refused — every redirect destination is re-validated. Combined with the default-on SSRF guard (no AWS metadata, no localhost pivot, no scheme downgrade, max 5 hops, DNS-rebinding multi-A check), the agent can hit only the hosts you declared. This is the critical piece for agents that pick URLs themselves.

Every one of these is a uniform, structured failure. You audit them by reviewing the `.perch` file — not by reading a Go service across 14 files.

---

## Three verticals — same shape

### 1. Kubernetes / infrastructure ops

The example above. Verbs the agent gets: `restart_pod`, `get_logs`, `scale_deployment`. Anything else is unreachable. Combined with `--no-network` and `--env KUBECONFIG,HOME`, the agent has no path to exfiltrate state or hit external services.

### 2. Customer-support actions

```capy
command refund_order
    description "Issue a refund (capped at $500)"
    arg order_id type string end
    arg amount   type float end
    arg reason   type string end
    do
        if amount > 500.0
            fail "amount > $500 needs a human"
        end
        let body = format '{"order_id":"${order_id}","amount":${amount},"reason":"${reason}"}'
        let resp = http_post "https://billing.internal/refund" body
        print "${resp}"
    end
end

command reset_password
    description "Send password-reset email"
    arg user_email type string end
    do
        if not regex_match "${user_email}" "^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\.[a-zA-Z]+$"
            fail "invalid email"
        end
        shell "support-cli reset-password --email='${user_email}'"
    end
end
```

Run as `perch-mcp --no-write --env BILLING_TOKEN -f support.perch`. The agent can refund but only up to $500. It can reset passwords but only for validly-shaped emails. It cannot do anything else, regardless of how cleverly it phrases its request.

### 3. Database queries (canned, parameterized)

```capy
command sales_for_region
    description "Last 30 days sales for one region"
    arg region type string description "lower-case region code (us-east, eu-west, …)" end
    do
        if not regex_match "${region}" "^[a-z]+-[a-z]+$"
            fail "invalid region"
        end
        shell_output `psql -h db -U readonly -c "SELECT sum(amount) FROM sales WHERE region='${region}' AND created_at > now() - interval '30 days'"`
    end
end
```

Run as `perch-mcp --no-network --env PGPASSWORD --no-write -f reports.perch`. The agent can query but only the canned queries you wrote. It cannot run raw SQL. There is no SQL-injection surface to write a filter for, because the agent never picks the query string.

---

## Pair with `--ask` for in-the-loop review

In high-stakes settings, the operator can be the gate even when the agent picks the verb:

```sh
perch --ask -f ops.perch restart_pod ns=prod pod=api-3
```

The agent proposed the action; the human sees `[1] shell cmd="kubectl -n prod delete pod api-3"` and answers `y`, `n`, `a`, or `q`. Halfway between "agent acts" and "agent suggests." The same `.perch` file works for both — no separate review code.

---

## Audit the file like you'd audit a config

```sh
perch --check ops.perch          # static validation
gh pr view 42                    # the diff IS the security review
```

`--check` rejects undeclared placeholders, missing args, type mismatches, calls to ops that don't exist. The file IS the policy; reading it IS the audit. A new colleague — or a security reviewer — can absorb the entire LLM-callable surface area in the time it takes to read a 50-line YAML config.

This is the property that makes perch genuinely cheap to ship: you can give someone the file and they know everything the agent can do.

---

## Setting it up — five minutes

```sh
# 1. Install the MCP server
go install github.com/luowensheng/perch/cmd/perch-mcp@latest

# 2. Write your ops.perch (use the perch skill or the language reference)
#    https://luowensheng.github.io/perch/language/

# 3. Validate it
perch --check ops.perch

# 4. Wire into your agent (Claude Desktop shown; OpenAI / Anthropic SDK
#    function-calling works the same — perch_list returns the schema)
cat ~/Library/Application\ Support/Claude/claude_desktop_config.json
{
  "mcpServers": {
    "ops": {
      "command": "perch-mcp",
      "args": [
        "--no-network",
        "--no-write",
        "--env", "KUBECONFIG,HOME",
        "-f", "/abs/path/to/ops.perch"
      ]
    }
  }
}

# 5. Restart Claude Desktop. Done.
```

Five steps, no backend service, no Docker image, no ingress, no secret manager wiring beyond what your shell already has via `${KUBECONFIG}` etc.

---

## When this is NOT the right tool

To be fair about what perch isn't:

- **Streaming responses to the LLM.** perch is request/response. If the LLM needs incremental output (e.g. "watch this build and tell me when it fails"), wrap the long-running thing in a command that polls + returns a structured summary.
- **Stateful sessions across LLM turns.** perch is stateless per-call. State lives in your files/databases/external systems — which is usually where it belongs anyway.
- **Building a public SaaS.** perch is for org-internal or agent-internal tool access. It doesn't replace your customer-facing API.
- **Anything that needs custom auth flows.** "Is this agent allowed to talk to this `perch-mcp` instance" is a question for the orchestration layer (Claude Desktop config, kubernetes-secret-injection, etc.), not perch itself.

For everything else — the broad case of "give an agent a fixed set of typed actions to perform" — this is dramatically less code than building it yourself.

---

## Summary

| | Custom backend | perch + `perch-mcp` |
|---|---|---|
| Lines of code to expose 8 actions to an LLM | 1k–3k | one `.perch` file (~50 lines) |
| What "audit" means | read a codebase | read the file |
| Where the security boundary lives | scattered across handlers/middleware/auth | the grammar + a few CLI flags |
| Where the LLM tool schema lives | hand-maintained per action | derived from the file |
| Time to add a new action | feature branch, PR, deploy | edit file, restart `perch-mcp` |
| Time to remove an action | same | delete the command block |
| Deploy footprint | service, secrets, ingress, dashboard | a binary on `$PATH` |
| Local replay | "spin up the backend on your laptop" | `perch <cmd>` |
| Restrict capabilities | code | composable `--no-*` flags + `--env` allowlist |

You're not getting fewer features. You're getting the *same* features with vastly less code to write and maintain — because perch already implemented the framework half (typing, dispatch, validation, restrictions, audit) and your file just declares the actions on top.

That's the value proposition: **a controlled LLM-action surface without backend infra.**
