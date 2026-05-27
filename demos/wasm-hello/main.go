// hello.wasm — a tiny WASI module that demonstrates perch's wasm_run
// capability gating. The module tries to access argv, env, the
// filesystem; what it can actually see is exactly what the .perch
// file declared.
//
// Build with:
//
//	GOOS=wasip1 GOARCH=wasm go build -o hello.wasm .
//
// Then run via the sibling commands.perch:
//
//	perch -f commands.perch demo
package main

import (
	"fmt"
	"os"
)

func main() {
	fmt.Println("── hello.wasm ────────────────────────────────")
	fmt.Println("argv:", os.Args)

	// Env visibility — only what wasm_env declared comes through.
	for _, name := range []string{"GREETING", "HOME", "SECRET", "PATH"} {
		if v, ok := os.LookupEnv(name); ok {
			fmt.Printf("env %s: %s\n", name, v)
		} else {
			fmt.Printf("env %s: (not visible — not in allowlist)\n", name)
		}
	}

	// FS visibility — only mounted paths are reachable. Mounts land at
	// /ro/<basename> (read-only) and /rw/<basename> (read-write) by
	// perch convention.
	probe := func(p string) {
		if _, err := os.Stat(p); err == nil {
			fmt.Printf("fs %s: visible\n", p)
		} else {
			fmt.Printf("fs %s: (not visible — not mounted)\n", p)
		}
	}
	probe("/ro/src")
	probe("/rw/bin")
	probe("/etc/passwd") // never visible — we never mount /etc
}
