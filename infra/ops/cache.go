package ops

// User-keyed body cache (Flavor A).
//
// Form:
//
//   cache "build-${target}-${sha256_file('go.sum')}" "24h"
//       shell "go build -o bin/${target} ./cmd"
//       let size = file_size "bin/${target}"
//   end
//
// First positional arg = cache key (after ${} interpolation by the
// interpreter, identical to any other op arg). Second = TTL duration.
// On miss: run the body, capture every `let X = …` binding that came
// out of it, persist the captures + a timestamp under
// ~/.cache/perch/blocks/<sha256(key)>.json. On hit within TTL: skip
// the body, replay the captured bindings into the current scope,
// continue.
//
// Honest framing (this is what docs/sandbox.md will say): perch does NOT
// hash the body's transitively-read inputs. The user picks the key, and
// the key is the contract. If they leave a stale input out of the key,
// they get stale cache. This is intentional — perch lacks the
// hermeticity needed for content-addressed caching (see ideas/05). The
// user-keyed model matches how GitHub Actions cache, Earthly --cache-id,
// and most practical caching layers actually work.

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"time"

	"github.com/luowensheng/perch/domain"
	"github.com/luowensheng/perch/infra/interpreter"
)

func registerCache(m map[string]interpreter.Handler) {
	m["cache"] = opCache
}

// cacheEntry is the on-disk representation. Format is versioned via
// `V` so future schema changes can be detected and old entries treated
// as cache misses rather than mis-parsed.
type cacheEntry struct {
	V         int            `json:"v"`
	Key       string         `json:"key"`
	StoredAt  time.Time      `json:"stored_at"`
	ExpiresAt time.Time      `json:"expires_at"`
	Bindings  map[string]any `json:"bindings"`
}

const cacheVersion = 1

func opCache(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	body, _ := args["_body"].([]domain.Op)
	key := argString(args, "key", "_0")
	if key == "" {
		return nil, fmt.Errorf("cache: missing key (first positional arg)")
	}
	ttlStr := argString(args, "ttl", "_1")
	ttl := 24 * time.Hour
	if ttlStr != "" {
		d, err := time.ParseDuration(ttlStr)
		if err != nil {
			return nil, fmt.Errorf("cache: invalid ttl %q: %w", ttlStr, err)
		}
		ttl = d
	}
	root, err := cacheRoot()
	if err != nil {
		// Cache directory unavailable — silently fall through to running
		// the body. A failed cache should never block real work.
		return nil, i.RunOps(body, b)
	}
	hash := sha256.Sum256([]byte(key))
	path := filepath.Join(root, hex.EncodeToString(hash[:])+".json")

	// Hit?
	if data, err := os.ReadFile(path); err == nil {
		var entry cacheEntry
		if err := json.Unmarshal(data, &entry); err == nil &&
			entry.V == cacheVersion &&
			entry.Key == key &&
			time.Now().Before(entry.ExpiresAt) {
			fmt.Fprintf(i.Stderr, "↪ cache hit: %s (replayed %d bindings, %s left)\n",
				truncateKey(key), len(entry.Bindings), time.Until(entry.ExpiresAt).Round(time.Second))
			for k, v := range entry.Bindings {
				b.Set(k, v)
			}
			return nil, nil
		}
	}

	// Miss: run the body and capture every binding it newly sets.
	before := snapshotVars(b)
	if err := i.RunOps(body, b); err != nil {
		return nil, err
	}
	captured := diffVars(before, b)
	entry := cacheEntry{
		V:         cacheVersion,
		Key:       key,
		StoredAt:  time.Now(),
		ExpiresAt: time.Now().Add(ttl),
		Bindings:  captured,
	}
	if err := os.MkdirAll(root, 0o755); err == nil {
		if data, err := json.Marshal(entry); err == nil {
			_ = os.WriteFile(path, data, 0o644)
		}
	}
	return nil, nil
}

// cacheRoot returns the path under which body-cache entries live. Each
// entry is one JSON file named by sha256(key).
func cacheRoot() (string, error) {
	base, err := os.UserCacheDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(base, "perch", "blocks"), nil
}

// snapshotVars copies the current Vars map. Used to diff before/after
// the body runs so only NEW or CHANGED bindings get cached.
func snapshotVars(b *interpreter.Bindings) map[string]any {
	out := make(map[string]any, len(b.Vars))
	for k, v := range b.Vars {
		out[k] = v
	}
	return out
}

// diffVars returns the bindings that were added or modified after the
// snapshot. Auto-bindings (os, arch, home, …) are filtered out so the
// cache file stays small and recognisable.
func diffVars(before map[string]any, b *interpreter.Bindings) map[string]any {
	skipPrefixes := map[string]bool{
		"os": true, "arch": true, "home": true, "home_dir": true,
		"config_dir": true, "cache_dir": true, "temp_dir": true,
		"data_dir": true, "exe_path": true, "exe_dir": true,
		"exe_name": true, "script_path": true, "script_dir": true,
		"user": true, "uid": true, "hostname": true, "pid": true,
		"now_unix": true, "cpu_count": true,
		"is_windows": true, "is_macos": true, "is_linux": true,
		"is_unix": true, "is_arm64": true, "is_amd64": true,
		"path_sep": true, "path_list_sep": true, "exe_ext": true,
		"null_device": true, "shell_name": true,
	}
	out := map[string]any{}
	for k, v := range b.Vars {
		if skipPrefixes[k] {
			continue
		}
		if prev, had := before[k]; !had || !equalAny(prev, v) {
			out[k] = v
		}
	}
	return out
}

// equalAny reports loose equality of two binding values. Numeric types
// are compared after a float widening so int(5) and float64(5) match —
// JSON round-tripping through cache files normalises everything to
// float64, and replays should compare equal pre/post.
func equalAny(a, b any) bool {
	if a == nil || b == nil {
		return a == b
	}
	af, aok := toFloatX(a)
	bf, bok := toFloatX(b)
	if aok && bok {
		return af == bf
	}
	return fmt.Sprintf("%v", a) == fmt.Sprintf("%v", b)
}

func toFloatX(v any) (float64, bool) {
	switch x := v.(type) {
	case float64:
		return x, true
	case float32:
		return float64(x), true
	case int:
		return float64(x), true
	case int64:
		return float64(x), true
	}
	return 0, false
}

// truncateKey shortens long keys for the cache-hit message.
func truncateKey(s string) string {
	if len(s) <= 60 {
		return s
	}
	return s[:57] + "..."
}
