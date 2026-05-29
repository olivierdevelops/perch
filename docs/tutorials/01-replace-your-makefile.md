# Tutorial 1 — Replace your Makefile

**Time:** 10 minutes. **You'll need:** a small Go project (or any project with a Makefile). **You'll end up with:** a `commands.perch` that's shorter, cross-platform, and drives both local dev and CI from the same file.

We'll convert this Makefile:

```makefile
APP_NAME := myapp
BIN_DIR  := ./bin
MAIN     := ./cmd/myapp

.PHONY: build test lint clean release ci

build:
	mkdir -p $(BIN_DIR)
	go build -ldflags='-s -w' -o $(BIN_DIR)/$(APP_NAME) $(MAIN)

test:
	go test -race ./...

lint:
	go vet ./...
	@which staticcheck > /dev/null && staticcheck ./... || true

clean:
	rm -rf $(BIN_DIR)

release: build
	GOOS=linux  go build -o $(BIN_DIR)/linux/$(APP_NAME)  $(MAIN)
	GOOS=darwin go build -o $(BIN_DIR)/darwin/$(APP_NAME) $(MAIN)
	# 🙃 windows here would need a second target with .exe and different syntax

ci: lint test release
```

## Step 1 — Scaffold

```sh
perch --init
```

This writes a starter `commands.perch`. Open it and clear the body — we're going to rewrite from scratch.

## Step 2 — Globals

Things you reference in multiple commands go in `globals`:

```capy
name    "myapp"
about   "Build, test, lint, release myapp"
version "0.1.0"

globals
    APP_NAME = "myapp"
    BIN_DIR  = "./bin"
    MAIN     = "./cmd/myapp"
end
```

While you're here, declare what this file needs from the host. A `requires`
block makes the dependency explicit and lets `perch --check` prove the file
will work *before* you run it — and refuses any undeclared shell bin at
runtime:

```capy
requires
    bin "go"                         # existence is verified at preflight
    bin "golangci-lint" optional
end
```

## Step 3 — `build`

In Make:
```makefile
build:
	mkdir -p $(BIN_DIR)
	go build -ldflags='-s -w' -o $(BIN_DIR)/$(APP_NAME) $(MAIN)
```

In perch:

```capy
command build
    description "Compile the binary"
    do
        mkdir "${BIN_DIR}"
        exec go build "-ldflags=-s -w" -o ${BIN_DIR}/${APP_NAME} ${MAIN}
    end
end
```

Try it:

```sh
perch build
ls bin/
```

Two improvements over Make:

- `mkdir` is a first-class op — works on Windows too. `mkdir -p` doesn't exist there.
- `${VAR}` and shell `$VAR` don't fight. perch substitutes before the shell sees anything.

## Step 4 — `test`, `lint`, `clean`

```capy
command test
    description "Run tests with race detection"
    do
        exec go test -race ./...
    end
end

command lint
    description "Run go vet plus staticcheck if available"
    do
        exec go vet ./...
        if exists "${HOME}/go/bin/staticcheck"
            exec ${HOME}/go/bin/staticcheck ./...
        end
    end
end

command clean
    description "Remove build artifacts"
    do
        rm "${BIN_DIR}"
        print "Cleaned ${BIN_DIR}/"
    end
end
```

Notice `if exists "..."`: this is what Make's `which staticcheck > /dev/null && ...` is trying to express, except now it's a real block op. The `|| true` ugly-hack is gone.

## Step 5 — `release` with cross-compile

In Make this was three near-identical lines. In perch it's one parameterised command + one `release` that calls it three times:

```capy
command build_for
    description "Compile for one specific target OS"

    arg target
        type string
        default "darwin"
        description "Target OS"
    end

    do
        mkdir "${BIN_DIR}/${target}"
        with_env "GOOS=${target}"
            exec go build "-ldflags=-s -w" -o ${BIN_DIR}/${target}/${APP_NAME} ${MAIN}
        end
    end
end

command release
    description "Cross-compile for all three OSes"
    do
        run build_for "-target=darwin"
        run build_for "-target=linux"
        run build_for "-target=windows"
    end
end
```

`run COMMAND` is a real op — no recursive-make tricks, no `$(MAKE) -C` weirdness.

## Step 6 — `ci`

```capy
command ci
    description "What CI runs: lint + test + release"
    do
        run lint
        run test
        run release
    end
end
```

And in `.github/workflows/ci.yml`:

```yaml
- run: perch ci
```

That's the whole CI job. The matrix lives in `commands.perch`, not in YAML.

## Step 7 — Reap the cross-platform benefit

The Make version silently broke on Windows. Let's prove perch's version doesn't. Add a Windows-aware lint:

```capy
command lint
    description "Run go vet plus staticcheck if available"
    do
        exec go vet ./...
        if os == "windows"
            if exists "${USERPROFILE}/go/bin/staticcheck.exe"
                exec ${USERPROFILE}/go/bin/staticcheck.exe ./...
            end
        end
        if os == "darwin"
            if exists "${HOME}/go/bin/staticcheck"
                exec ${HOME}/go/bin/staticcheck ./...
            end
        end
        if os == "linux"
            if exists "${HOME}/go/bin/staticcheck"
                exec ${HOME}/go/bin/staticcheck ./...
            end
        end
    end
end
```

Same file. Three platforms. Zero Makefile-per-OS dance.

## Bonus — your team's muscle memory carries over

`perch --init` writes a shebang at the top of `commands.perch` and sets the file executable. That means your team can invoke commands the same way they invoked Make targets:

```sh
# Before (Make):
make build
make test
make ci

# After (perch):
./commands.perch build
./commands.perch test
./commands.perch ci
```

Same shape, same muscle memory, plus all the things Make didn't have: typed args, per-command `--help`, `--check` static validation, `--scan` security audit, a web UI, MCP for AI agents. If you prefer the `perch` prefix that's also still there (`perch build`, `perch -f commands.perch build`) — pick whichever your team finds cleaner.

## What you learned

- Globals replace Makefile variables. Interpolation is `${name}` not `$(name)`.
- Each Make target maps to a `command NAME ... end` block.
- Ops (`mkdir`, `rm`, `if exists "..."`, `if os == "..."`) replace shell incantations and platform-conditional Makefile-snippet hackery.
- `run COMMAND` replaces recursive Make.
- One file drives both local dev and CI.
- The shebang + `+x` permissions make `./commands.perch test` work — no `perch` prefix required once `perch` is on `$PATH`.

## Next

→ Tutorial 2: [Ship a tool](02-ship-a-tool.md) — bundle a `commands.perch` into a portable single-file binary with `perch --build`.
