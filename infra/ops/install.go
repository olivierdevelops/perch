// Install / uninstall / build helpers — the ops most needed when a
// .perch file is shipped as a binary that has to bootstrap itself on
// the recipient's machine. Everything here is cross-platform; on
// Windows the helpers degrade gracefully (e.g. add_to_path prints
// instructions when it can't safely edit a shell rc).
package ops

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"

	"github.com/luowensheng/perch/infra/interpreter"
)

func registerInstall(m map[string]interpreter.Handler) {
	m["which"] = opWhich
	m["has_bin"] = opHasBin
	m["bin_version"] = opBinVersion

	m["path_contains"] = opPathContains
	m["shell_rc_path"] = opShellRcPath
	m["add_to_path"] = opAddToPath
	m["link_into_path"] = opLinkIntoPath

	m["detect_pkg_mgr"] = opDetectPkgMgr
	m["pkg_install"] = opPkgInstall
	m["pkg_installed"] = opPkgInstalled
	m["pkg_uninstall"] = opPkgUninstall

	m["is_admin"] = opIsAdmin
	m["is_ci"] = opIsCI
	m["is_tty"] = opIsTTY
}

// opWhich returns the absolute path to BIN on PATH, or "" if not found.
// `if has_bin "python3"` is the truthy form; use `let p = which "python3"`
// when you need the full path.
func opWhich(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	name := argString(args, "name", "_0")
	if name == "" {
		return "", nil
	}
	p, err := exec.LookPath(name)
	if err != nil {
		return "", nil
	}
	if abs, err := filepath.Abs(p); err == nil {
		return abs, nil
	}
	return p, nil
}

func opHasBin(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	name := argString(args, "name", "_0")
	if name == "" {
		return false, nil
	}
	_, err := exec.LookPath(name)
	return err == nil, nil
}

// opBinVersion runs `BIN --version` and returns the trimmed stdout.
// Empty string on failure (no error — callers usually want a fallback).
func opBinVersion(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	name := argString(args, "name", "_0")
	if err := CheckSubprocessBin(i, name); err != nil {
		return "", err
	}
	flag := argString(args, "flag", "_1")
	if flag == "" {
		flag = "--version"
	}
	out, err := exec.Command(name, flag).CombinedOutput()
	if err != nil {
		return "", nil
	}
	return strings.TrimSpace(string(out)), nil
}

func opPathContains(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	dir := argString(args, "dir", "_0")
	if dir == "" {
		return false, nil
	}
	want, _ := filepath.Abs(dir)
	sep := string(os.PathListSeparator)
	for _, p := range strings.Split(os.Getenv("PATH"), sep) {
		if got, err := filepath.Abs(p); err == nil && got == want {
			return true, nil
		}
	}
	return false, nil
}

// opShellRcPath returns the best-guess shell rc file for the current
// user — ~/.zshrc, ~/.bashrc, $PROFILE.ps1, etc. Picks based on $SHELL.
// Returns "" on Windows (use environment variable APIs instead).
func opShellRcPath(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	if runtime.GOOS == "windows" {
		return "", nil
	}
	h, err := os.UserHomeDir()
	if err != nil {
		return "", err
	}
	shell := filepath.Base(os.Getenv("SHELL"))
	switch shell {
	case "zsh":
		return filepath.Join(h, ".zshrc"), nil
	case "fish":
		return filepath.Join(h, ".config", "fish", "config.fish"), nil
	case "bash":
		// macOS conventionally uses .bash_profile for login shells.
		if runtime.GOOS == "darwin" {
			return filepath.Join(h, ".bash_profile"), nil
		}
		return filepath.Join(h, ".bashrc"), nil
	default:
		return filepath.Join(h, ".profile"), nil
	}
}

// opAddToPath idempotently appends `export PATH="DIR:$PATH"` to the
// user's shell rc if DIR is not already on PATH and the line isn't
// already there. On Windows it prints instructions instead of editing
// the registry (intentional — registry edits surprise people).
func opAddToPath(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	dir := argString(args, "dir", "_0")
	if dir == "" {
		return false, fmt.Errorf("add_to_path: dir is required")
	}
	abs, _ := filepath.Abs(dir)
	if alreadyOnPath(abs) {
		return false, nil
	}
	if runtime.GOOS == "windows" {
		fmt.Fprintf(i.Stdout, "To add %q to PATH, run:\n  setx PATH \"%%PATH%%;%s\"\n", abs, abs)
		return false, nil
	}
	rc, err := opShellRcPath(i, b, args)
	if err != nil {
		return false, err
	}
	rcPath := interpreter.ToStringValue(rc)
	if rcPath == "" {
		return false, nil
	}
	shell := filepath.Base(os.Getenv("SHELL"))
	var line string
	if shell == "fish" {
		line = fmt.Sprintf("set -gx PATH %s $PATH", abs)
	} else {
		line = fmt.Sprintf("export PATH=\"%s:$PATH\"", abs)
	}
	added, err := ensureLineInFile(rcPath, line)
	if err != nil {
		return false, err
	}
	if added {
		fmt.Fprintf(i.Stdout, "Added %q to %s — start a new shell or `source %s`.\n", abs, rcPath, rcPath)
	}
	return added, nil
}

// opLinkIntoPath creates a symlink (or copies, on Windows) so SRC is
// reachable as `BASENAME(SRC)` inside DIR. Useful for "drop a launcher
// in ~/.local/bin" patterns.
func opLinkIntoPath(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	src := argString(args, "src", "_0")
	dir := argString(args, "dir", "_1")
	if src == "" || dir == "" {
		return nil, fmt.Errorf("link_into_path: src and dir are required")
	}
	if err := os.MkdirAll(dir, 0755); err != nil {
		return nil, err
	}
	dst := filepath.Join(dir, filepath.Base(src))
	_ = os.Remove(dst)
	if runtime.GOOS == "windows" {
		// Copy instead of symlink: Windows symlinks need elevation.
		return dst, copyFile(src, dst)
	}
	return dst, os.Symlink(src, dst)
}

// opDetectPkgMgr returns "brew" / "apt" / "dnf" / "yum" / "pacman" /
// "apk" / "zypper" / "choco" / "winget" / "scoop" — the first one
// found on PATH. Empty string if none.
func opDetectPkgMgr(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	candidates := pkgMgrCandidates()
	for _, c := range candidates {
		if _, err := exec.LookPath(c); err == nil {
			return c, nil
		}
	}
	return "", nil
}

func pkgMgrCandidates() []string {
	switch runtime.GOOS {
	case "darwin":
		return []string{"brew", "port"}
	case "linux":
		return []string{"apt", "dnf", "yum", "pacman", "apk", "zypper"}
	case "windows":
		return []string{"winget", "choco", "scoop"}
	}
	return nil
}

// opPkgInstall installs NAME using the detected package manager. The
// package name MUST match the manager's catalog — there's no
// cross-manager name remapping. If you need that, drive it via your
// own `if os == "..."` blocks.
func opPkgInstall(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	name := argString(args, "name", "_0")
	if name == "" {
		return nil, fmt.Errorf("pkg_install: name is required")
	}
	mgrV, _ := opDetectPkgMgr(i, b, args)
	mgr := interpreter.ToStringValue(mgrV)
	cmd := pkgInstallCmd(mgr, name)
	if cmd == nil {
		return nil, fmt.Errorf("pkg_install: no package manager found on PATH")
	}
	if err := CheckSubprocessBin(i, cmd[0]); err != nil {
		return nil, err
	}
	c := exec.Command(cmd[0], cmd[1:]...)
	c.Stdout = i.Stdout
	c.Stderr = i.Stderr
	c.Stdin = i.Stdin
	return nil, c.Run()
}

func opPkgUninstall(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	name := argString(args, "name", "_0")
	if name == "" {
		return nil, fmt.Errorf("pkg_uninstall: name is required")
	}
	mgrV, _ := opDetectPkgMgr(i, b, args)
	mgr := interpreter.ToStringValue(mgrV)
	cmd := pkgUninstallCmd(mgr, name)
	if cmd == nil {
		return nil, fmt.Errorf("pkg_uninstall: no package manager found on PATH")
	}
	if err := CheckSubprocessBin(i, cmd[0]); err != nil {
		return nil, err
	}
	c := exec.Command(cmd[0], cmd[1:]...)
	c.Stdout = i.Stdout
	c.Stderr = i.Stderr
	c.Stdin = i.Stdin
	return nil, c.Run()
}

func opPkgInstalled(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	name := argString(args, "name", "_0")
	mgrV, _ := opDetectPkgMgr(i, b, args)
	mgr := interpreter.ToStringValue(mgrV)
	cmd := pkgInstalledCmd(mgr, name)
	if cmd == nil {
		return false, nil
	}
	if err := CheckSubprocessBin(i, cmd[0]); err != nil {
		return false, err
	}
	return exec.Command(cmd[0], cmd[1:]...).Run() == nil, nil
}

func pkgInstallCmd(mgr, name string) []string {
	switch mgr {
	case "brew":
		return []string{"brew", "install", name}
	case "apt":
		return []string{"sudo", "apt-get", "install", "-y", name}
	case "dnf":
		return []string{"sudo", "dnf", "install", "-y", name}
	case "yum":
		return []string{"sudo", "yum", "install", "-y", name}
	case "pacman":
		return []string{"sudo", "pacman", "-S", "--noconfirm", name}
	case "apk":
		return []string{"sudo", "apk", "add", name}
	case "zypper":
		return []string{"sudo", "zypper", "install", "-y", name}
	case "winget":
		return []string{"winget", "install", "--silent", "-e", "--id", name}
	case "choco":
		return []string{"choco", "install", "-y", name}
	case "scoop":
		return []string{"scoop", "install", name}
	}
	return nil
}

func pkgUninstallCmd(mgr, name string) []string {
	switch mgr {
	case "brew":
		return []string{"brew", "uninstall", name}
	case "apt":
		return []string{"sudo", "apt-get", "remove", "-y", name}
	case "dnf", "yum":
		return []string{"sudo", mgr, "remove", "-y", name}
	case "pacman":
		return []string{"sudo", "pacman", "-R", "--noconfirm", name}
	case "apk":
		return []string{"sudo", "apk", "del", name}
	case "zypper":
		return []string{"sudo", "zypper", "remove", "-y", name}
	case "winget":
		return []string{"winget", "uninstall", "--silent", "-e", "--id", name}
	case "choco":
		return []string{"choco", "uninstall", "-y", name}
	case "scoop":
		return []string{"scoop", "uninstall", name}
	}
	return nil
}

func pkgInstalledCmd(mgr, name string) []string {
	switch mgr {
	case "brew":
		return []string{"brew", "list", name}
	case "apt":
		return []string{"dpkg", "-s", name}
	case "dnf", "yum":
		return []string{"rpm", "-q", name}
	case "pacman":
		return []string{"pacman", "-Qi", name}
	case "apk":
		return []string{"apk", "info", "-e", name}
	}
	return nil
}

// opIsAdmin: euid 0 on unix; on Windows, a probe via `net session`.
func opIsAdmin(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	if runtime.GOOS == "windows" {
		return exec.Command("net", "session").Run() == nil, nil
	}
	return os.Geteuid() == 0, nil
}

// opIsCI returns true if any common CI env var is set.
func opIsCI(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	for _, k := range []string{"CI", "GITHUB_ACTIONS", "GITLAB_CI", "CIRCLECI", "TRAVIS", "BUILDKITE", "JENKINS_URL", "TEAMCITY_VERSION"} {
		if os.Getenv(k) != "" {
			return true, nil
		}
	}
	return false, nil
}

// opIsTTY: stdout looks like a terminal (vs piped / redirected).
func opIsTTY(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	fi, err := os.Stdout.Stat()
	if err != nil {
		return false, nil
	}
	return (fi.Mode() & os.ModeCharDevice) != 0, nil
}

// shared helpers (also used from files.go) ----------------------------

func alreadyOnPath(abs string) bool {
	sep := string(os.PathListSeparator)
	for _, p := range strings.Split(os.Getenv("PATH"), sep) {
		if got, err := filepath.Abs(p); err == nil && got == abs {
			return true
		}
	}
	return false
}

// ensureLineInFile appends LINE to PATH if not already present.
// Returns true if the line was added (false if it was already there).
func ensureLineInFile(path, line string) (bool, error) {
	data, err := os.ReadFile(path)
	if err != nil && !os.IsNotExist(err) {
		return false, err
	}
	if containsLine(string(data), line) {
		return false, nil
	}
	f, err := os.OpenFile(path, os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0644)
	if err != nil {
		return false, err
	}
	defer f.Close()
	prefix := ""
	if len(data) > 0 && data[len(data)-1] != '\n' {
		prefix = "\n"
	}
	if _, err := f.WriteString(prefix + line + "\n"); err != nil {
		return false, err
	}
	return true, nil
}

func containsLine(haystack, line string) bool {
	for _, l := range strings.Split(haystack, "\n") {
		if strings.TrimRight(l, "\r") == line {
			return true
		}
	}
	return false
}

func copyFile(src, dst string) error {
	in, err := os.Open(src)
	if err != nil {
		return err
	}
	defer in.Close()
	out, err := os.Create(dst)
	if err != nil {
		return err
	}
	defer out.Close()
	_, err = out.ReadFrom(in)
	return err
}
