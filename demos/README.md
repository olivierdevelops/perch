# perch demos

Each subfolder is a self-contained `commands.perch` you can copy into your own project. Run any of them with `perch <command>` from inside the demo folder.

| Demo | What it shows | Try |
|---|---|---|
| [01-hello](01-hello/) | Globals, args, `let`, `print`. The 30-second intro. | `perch hello -name=World` |
| [02-cross-platform-setup](02-cross-platform-setup/) | `if_os` branching that installs deps via brew/apt/choco. | `perch setup` |
| [03-go-project](03-go-project/) | Build, test, lint a Go project. Could replace your Makefile. | `perch build -target=linux` |
| [04-portable-cli](04-portable-cli/) | Bundle a `commands.perch` into a single portable binary via `perch --build`. The killer feature. | `perch --build -o greet && ./greet hello -name=Alice` |

More demos welcome — open a PR adding a folder under `demos/` with a `commands.perch` and a short `README.md`.
