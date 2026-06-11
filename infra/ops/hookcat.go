package ops

import "sync"

// Hook categories — the capability classes a `hooks` block may target
// (`before write …`, `after net …`). They mirror the `requires` capabilities so
// the vocabulary stays small: exec · net · write · read · env. An op can belong
// to more than one (e.g. `cp` is both read and write).
const (
	HookCatExec  = "exec"  // any subprocess spawn (shell + exec + process mgmt)
	HookCatNet   = "net"   // any network op
	HookCatWrite = "write" // any filesystem-mutating op
	HookCatRead  = "read"  // any filesystem-reading op
	HookCatEnv   = "env"   // any environment read/write
)

var (
	hookCatOnce sync.Once
	hookCatMap  map[string][]string
)

// readKinds / envKinds aren't covered by the restrict flags (there is no
// --no-read / --no-env), so list them explicitly. Source of truth is the
// capability-gating table.
var hookReadKinds = []string{
	"read_file", "exists", "is_dir", "is_file", "file_size", "list_dir",
	"walk_dir", "read_link", "sha256_file", "sha1_file", "md5_file", "glob",
	"verify_sha256", "cp", "mv", "copy_dir",
}

var hookEnvKinds = []string{
	"get_env", "set_env", "unset_env", "env_has", "env_default",
}

func buildHookCat() map[string][]string {
	m := map[string]map[string]bool{} // kind -> set(categories)
	add := func(kind, cat string) {
		if m[kind] == nil {
			m[kind] = map[string]bool{}
		}
		m[kind][cat] = true
	}
	for _, k := range restrictBlocks[RestrictNoShell] {
		add(k, HookCatExec)
	}
	for _, k := range restrictBlocks[RestrictNoSubprocess] {
		add(k, HookCatExec)
	}
	for _, k := range restrictBlocks[RestrictNoNetwork] {
		add(k, HookCatNet)
	}
	for _, k := range restrictBlocks[RestrictNoWrite] {
		add(k, HookCatWrite)
	}
	for _, k := range hookReadKinds {
		add(k, HookCatRead)
	}
	for _, k := range hookEnvKinds {
		add(k, HookCatEnv)
	}
	out := map[string][]string{}
	for kind, set := range m {
		for cat := range set {
			out[kind] = append(out[kind], cat)
		}
	}
	return out
}

// HookCategoryOf returns the capability categories an op kind belongs to (may be
// empty for a pure op, or multiple for a read+write op). Wired into the
// interpreter so a `hooks before write …` line matches every write op without
// the interpreter needing to import the ops package.
func HookCategoryOf(kind string) []string {
	hookCatOnce.Do(func() { hookCatMap = buildHookCat() })
	return hookCatMap[kind]
}

// HookCategories returns the set of valid category names a hooks line may
// target — used by `--check` to validate a hook's target.
func HookCategories() []string {
	return []string{HookCatExec, HookCatNet, HookCatWrite, HookCatRead, HookCatEnv}
}
