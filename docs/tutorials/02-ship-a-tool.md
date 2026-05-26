# Tutorial 2 — Ship a tool

**Time:** 10 minutes. **You'll end up with:** a single binary you can email, SCP, or drop on a server. It has no runtime dependencies. Recipients don't need perch installed.

## The setup

Suppose you maintain a small team operations CLI: `ops backup`, `ops restart-api`, `ops check-disk`. Today it's a folder of bash scripts. Half the team has the folder cloned; half doesn't. There's no help text. Onboarding takes 20 minutes.

We're going to fold the whole thing into one `commands.capy` and ship the result as `ops`.

## Step 1 — Write the commands

`commands.capy`:

```capy
name    "ops"
about   "Internal ops CLI for the platform team"
version "1.0.0"

globals
    DB_HOST   = "db-primary.internal"
    BACKUP_S3 = "s3://backups-bucket"
end

command backup
    description "Snapshot the primary DB to S3"
    do
        let stamp = now "unix"
        shell "pg_dump -h {{DB_HOST}} -Fc > /tmp/backup-{{stamp}}.dump"
        shell "aws s3 cp /tmp/backup-{{stamp}}.dump {{BACKUP_S3}}/db-{{stamp}}.dump"
        rm "/tmp/backup-{{stamp}}.dump"
        print "✓ uploaded as db-{{stamp}}.dump"
    end
end

command restart-api
    description "Roll-restart the api deployment"
    do
        shell "kubectl rollout restart deployment/api -n prod"
        shell "kubectl rollout status deployment/api -n prod"
        print "✓ api restarted"
    end
end

command check-disk
    description "Print disk usage on each api node"
    do
        shell "kubectl get nodes -l role=api -o name | xargs -I {} kubectl debug {} --image=busybox -- df -h"
    end
end

catch unknown
    do
        print "Unknown command: {{unknown}}"
        print "Try: backup | restart-api | check-disk"
        exit 1
    end
end
```

You could already invoke this as `perch backup`, `perch restart-api`, etc. But we want a standalone tool the team can run without thinking about perch.

## Step 2 — Build the binary

```sh
perch --build -o ops
```

That's it. The output `ops` is a self-contained executable:

```sh
file ./ops
# → Mach-O 64-bit executable arm64    (or ELF / PE depending on your host)

./ops --version    # → 1.0.0
./ops --help       # → lists backup, restart-api, check-disk
./ops backup       # → runs the dump+upload pipeline
./ops blorp        # → falls into your `catch unknown`
```

Notice:

- `--version` returns the program's `version` field, **not** perch's version.
- `--help` is auto-generated from `description` lines.
- `-f any.capy` is ignored — the embedded program always wins.

## Step 3 — Distribute

Same-OS, same-arch is the only constraint:

```sh
# Email it
mutt -s 'new ops binary' -a ops team@example.com

# SCP to bastion
scp ops bastion.prod:/usr/local/bin/

# Drop in artifact storage
aws s3 cp ops s3://ops-binaries/ops-v1.0.0

# Distribute via npm-like channel? Coming soon (`perch share`)
```

The recipient runs `./ops backup` immediately. No perch install. No Go toolchain.

## Step 4 — How it works (briefly)

`perch --build`:

1. Reads `commands.capy`, parses it via the same capy pipeline `perch` uses.
2. Marshals the resulting `domain.Program` as JSON.
3. Copies the running `perch` binary to your output path.
4. Appends the JSON + a footer: `<8 bytes length><8 bytes magic "PRCHEMB1">`.

When the resulting binary starts:

1. It reads its own last 16 bytes.
2. If the magic matches, it seeks back, loads the embedded JSON, and runs against that program.
3. If not, it behaves like normal perch.

Full format spec: [docs/embedding.md](../embedding.md).

## Step 5 — Update flow

When you change `commands.capy`:

```sh
# Edit commands.capy
$EDITOR commands.capy

# Rebuild — strips the old embedded program automatically
perch --build -o ops

# Ship the new binary
```

Re-`--build` is idempotent. No accumulating layers.

## Limitations

- **Same OS / arch only.** Building on macOS arm64 produces a Mach-O arm64 binary; you can't target Linux from there. Cross-compile is on the roadmap.
- **Op catalog is frozen at build time.** The binary uses the ops in whatever perch you `--build`'ed from. Adding new ops to perch later doesn't retroactively give old binaries those ops — rebuild.
- **No code signing built-in.** For distribution outside your trust boundary, sign with `codesign` (macOS) or `signtool` (Windows).

## What you learned

- `perch --build -o NAME` produces a portable single-file binary.
- The embedded `Program` is JSON; the runtime is a copy of perch.
- The output binary's `--help` and `--version` reflect the embedded program.
- Same file drives both `perch <cmd>` (dev) and `./ops <cmd>` (shipped).

## Next

→ Tutorial 3: [Cross-platform installer](03-cross-platform-installer.md) — write one `commands.capy` that installs deps on macOS / Linux / Windows from the same source.
