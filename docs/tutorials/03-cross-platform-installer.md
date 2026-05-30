# Tutorial 3 — Cross-platform installer

**Time:** 10 minutes. **You'll end up with:** one `commands.perch` that bootstraps a fresh dev machine — installs deps, sets env vars, fetches config — and works on macOS, Linux, and Windows from the same source.

## The problem

Onboarding a new engineer involves:

- Installing 6–10 system packages.
- Cloning some private configs.
- Setting env vars in their shell profile.

Today this is usually:

- A README with instructions that drift.
- A bash script that breaks on Windows.
- A PowerShell script that nobody on the macOS team reads.

We'll fold all three into one `commands.perch`.

## Step 1 — Sketch the install command

```capy
name    "dev-setup"
about   "Bootstrap a fresh dev machine"
version "0.1.0"

command setup
    description "Install dependencies, fetch config, set env"
    do
        install_packages
        fetch_config
        setup_env
        print "✓ Setup complete. Restart your shell."
    end
end
```

`run COMMAND` keeps things composable. We'll fill in the sub-commands.

## Step 2 — Cross-platform package install

```capy
command install_packages
    description "Install system packages for the current OS"
    do
        if os == "darwin"
            print "macOS — using Homebrew"
            exec brew install jq ripgrep watchexec gh
        end

        if os == "linux"
            print "Linux — using apt"
            exec sudo apt-get update
            exec sudo apt-get install -y jq ripgrep gh
            print "(watchexec is not in apt; install via cargo)"
        end

        if os == "windows"
            print "Windows — using Chocolatey"
            exec choco install jq ripgrep watchexec gh -y
        end
    end
end
```

Three `if os == "..."` blocks. Each runs *only* on the matching OS. The other two are skipped, not even attempted.

## Step 3 — Fetch shared config

Suppose the team keeps shared shell config in a private GitHub repo. Cloning it from each user's machine:

```capy
command fetch_config
    description "Clone team-config into ~/.team-config"
    do
        if exists "${HOME}/.team-config"
            print "team-config already cloned; pulling latest"
            cd "${HOME}/.team-config"
            exec git pull
        end
        if os == "windows"
            if exists "${USERPROFILE}/.team-config"
                cd "${USERPROFILE}/.team-config"
                exec git pull
            end
        end
        # Clone if missing
        if exists "${HOME}/.team-config"
            print "(already present)"
        end
    end
end
```

That `if exists "X" … end` / inverse pair is the perch idiom for "do X if Y else do Z." (A formal `else` is on the roadmap — for now, two complementary `if` blocks do the job. Use `let e = exists "PATH"` followed by `if not e` for the inverse branch.)

## Step 4 — Per-OS env files

Different shells, different files. Use `write_file` so the content is identical across platforms:

```capy
command setup_env
    description "Append PERCH_* env vars to the shell rc file"
    do
        if os == "darwin"
            write_file "${HOME}/.zshrc.perch" "export PERCH_HOME=$HOME/.team-config\nexport EDITOR=code\n"
            print "Add to your .zshrc: source ~/.zshrc.perch"
        end
        if os == "linux"
            write_file "${HOME}/.bashrc.perch" "export PERCH_HOME=$HOME/.team-config\nexport EDITOR=code\n"
            print "Add to your .bashrc: source ~/.bashrc.perch"
        end
        if os == "windows"
            exec setx PERCH_HOME "%USERPROFILE%\.team-config"
            exec setx EDITOR code
        end
    end
end
```

`write_file` and `setx` differ per OS but are surfaced as straight ops, no shell quoting drama.

## Step 5 — Test it on this machine

```sh
perch setup
```

Watch only the blocks for your OS run. The others silently skip — they're not errors.

## Step 6 — Ship it as a binary

The crown jewel: bundle this so a new hire runs **one binary** with **zero perch install**:

```sh
perch --build -o team-bootstrap
```

Distribute via your preferred channel (S3, GitHub Releases, an internal package server). New hire runs:

```sh
curl -fsSL https://internal/team-bootstrap -o ./bootstrap && chmod +x ./bootstrap && ./bootstrap setup
```

That's the onboarding doc.

## Step 7 — Diagnose

If something fails partway through, the user can re-run individual sub-commands:

```sh
./team-bootstrap install_packages    # just the package install
./team-bootstrap fetch_config        # just the config clone
./team-bootstrap setup_env           # just the env-var step
```

Because each sub-command is independent, partial recovery is straightforward.

## Step 8 — Future-proof for new platforms

When you add support for Fedora (apt → dnf), you don't touch the `setup` command. You add an extra `if os == "linux"` distinction inside `install_packages`:

```capy
if os == "linux"
    if exists "/etc/fedora-release"
        exec sudo dnf install -y jq ripgrep
    end
    if exists "/etc/debian_version"
        exec sudo apt-get install -y jq ripgrep
    end
end
```

The shape stays clean as the project grows.

## What you learned

- `if os == "..."` + `if arch == "..."` make cross-platform conditionals first-class.
- `run NAME` composes commands without recursion or duplication.
- `write_file` cleanly handles per-OS config files.
- The whole installer ships as one binary via `perch --build`.

> **Declare what it needs.** Because this installer shells out to per-OS tools, add a `requires` block so the file states its external surface and `perch --check` can verify it before anything runs. The OS-specific installers are `optional` (only one exists per host):
>
> ```capy
> requires
>     bin "git"
>     bin "brew"  optional        # macOS
>     bin "sudo"  optional        # linux
>     bin "choco" optional        # windows
>     write "${home_dir}/.config"
> end
> ```
>
> With the block present, every external op verifies it immediately before running. See [requires.md](../requires.md) and [capability-gating.md](../capability-gating.md).

## Where to go next

- Browse the [op catalog](../op-reference.md) for the full vocabulary.
- The [demos folder](https://github.com/luowensheng/perch/tree/main/demos) has variations of these patterns.
- File new op requests as issues — most ops are a single Go function and a `lib.capy` entry.
