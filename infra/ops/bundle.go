package ops

// Embedded-payload ops — only meaningful inside a fat binary built with
// `perch --build --include <path>`. Three ops:
//
//   bundle_hash                     → string  (sha256 of the embedded tar.gz)
//   bundle_dir                      → string  (lazy-extracted path; cached for the process)
//   bundle_extract "DST"            → string  (extract to DST, return DST)
//
// `bundle_dir` is the workhorse for the "install a Python / JS project
// from one binary" pattern: extract to a content-addressable cache
// (`$HOME/.cache/perch/<hash>/`), then run the project's own install
// script from there.

import (
	"archive/tar"
	"bytes"
	"compress/gzip"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"
	"sync"

	"github.com/luowensheng/perch/infra/interpreter"
)

var (
	bundleMu      sync.Mutex
	bundleArchive []byte
	bundleHash    string

	bundleExtractOnce sync.Once
	bundleExtractDir  string
	bundleExtractErr  error
)

// SetBundle is called by the orchestrator at startup with the bundle
// payload (or nil if the binary doesn't include one). Safe to call once.
func SetBundle(archive []byte, hash string) {
	bundleMu.Lock()
	defer bundleMu.Unlock()
	bundleArchive = archive
	bundleHash = hash
}

func registerBundle(m map[string]interpreter.Handler) {
	m["bundle_hash"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		bundleMu.Lock()
		defer bundleMu.Unlock()
		if bundleArchive == nil {
			return nil, fmt.Errorf("no embedded bundle (build with `perch --build --include <path>`)")
		}
		return bundleHash, nil
	}
	m["bundle_dir"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		bundleMu.Lock()
		if bundleArchive == nil {
			bundleMu.Unlock()
			// No bundle (running from a .perch file rather than a built
			// binary). Fall back to script_dir so a single .perch file
			// can use ${bundle_dir} uniformly in both modes.
			if sd, ok := b.Lookup("script_dir"); ok && sd != "" {
				return sd, nil
			}
			return nil, fmt.Errorf("no embedded bundle and script_dir is empty — provide a file via -f")
		}
		bundleMu.Unlock()
		bundleExtractOnce.Do(func() {
			dir, err := os.MkdirTemp("", "perch-bundle-"+shortHash(bundleHash)+"-")
			if err != nil {
				bundleExtractErr = err
				return
			}
			if err := extractTarGz(bundleArchive, dir); err != nil {
				bundleExtractErr = err
				return
			}
			bundleExtractDir = dir
		})
		return bundleExtractDir, bundleExtractErr
	}
	m["bundle_extract"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		bundleMu.Lock()
		if bundleArchive == nil {
			bundleMu.Unlock()
			return nil, fmt.Errorf("no embedded bundle")
		}
		archive := bundleArchive
		bundleMu.Unlock()
		dst := argString(args, "dst", "_0")
		if dst == "" {
			return nil, fmt.Errorf("bundle_extract: missing destination")
		}
		if err := os.MkdirAll(dst, 0755); err != nil {
			return nil, err
		}
		return dst, extractTarGz(archive, dst)
	}
}

// BundleReadFile streams the embedded tar.gz looking for a single entry
// matching `path` (relative to the bundle root, no leading slash, no
// `..` segments) and returns its bytes. Used by `wasm_run "bundle:…"`
// to run modules directly from the embedded archive without extracting
// to disk. Returns (nil, false, nil) if no bundle is loaded;
// (nil, true, err) if the bundle is present but the path isn't found
// or read failed.
func BundleReadFile(path string) ([]byte, bool, error) {
	bundleMu.Lock()
	archive := bundleArchive
	bundleMu.Unlock()
	if archive == nil {
		return nil, false, nil
	}
	// Normalize: strip leading "./" and any leading slash; reject ".." segments.
	path = strings.TrimPrefix(path, "./")
	path = strings.TrimPrefix(path, "/")
	if strings.Contains(path, "..") {
		return nil, true, fmt.Errorf("bundle: path traversal rejected: %q", path)
	}
	gz, err := gzip.NewReader(bytes.NewReader(archive))
	if err != nil {
		return nil, true, fmt.Errorf("bundle: gzip: %w", err)
	}
	defer gz.Close()
	tr := tar.NewReader(gz)
	for {
		hdr, err := tr.Next()
		if err == io.EOF {
			return nil, true, fmt.Errorf("bundle: entry %q not found", path)
		}
		if err != nil {
			return nil, true, fmt.Errorf("bundle: tar: %w", err)
		}
		if hdr.Typeflag != tar.TypeReg {
			continue
		}
		// Match exact (post-strip) or with a leading "./".
		name := strings.TrimPrefix(hdr.Name, "./")
		if name == path {
			buf := make([]byte, hdr.Size)
			if _, err := io.ReadFull(tr, buf); err != nil {
				return nil, true, fmt.Errorf("bundle: read %q: %w", path, err)
			}
			return buf, true, nil
		}
	}
}

// BundleHash returns the sha256 of the embedded archive (or "" if none).
// Used by callers that want to key a cache on the bundle's identity
// without re-hashing each entry.
func BundleHash() string {
	bundleMu.Lock()
	defer bundleMu.Unlock()
	return bundleHash
}

func shortHash(h string) string {
	if len(h) >= 12 {
		return h[:12]
	}
	return h
}

func extractTarGz(archive []byte, dst string) error {
	gz, err := gzip.NewReader(bytes.NewReader(archive))
	if err != nil {
		return fmt.Errorf("gzip: %w", err)
	}
	defer gz.Close()
	tr := tar.NewReader(gz)
	for {
		hdr, err := tr.Next()
		if err == io.EOF {
			return nil
		}
		if err != nil {
			return err
		}
		// Refuse path-traversal entries.
		if strings.Contains(hdr.Name, "..") {
			continue
		}
		out := filepath.Join(dst, hdr.Name)
		switch hdr.Typeflag {
		case tar.TypeDir:
			if err := os.MkdirAll(out, os.FileMode(hdr.Mode)); err != nil {
				return err
			}
		case tar.TypeReg:
			if err := os.MkdirAll(filepath.Dir(out), 0755); err != nil {
				return err
			}
			f, err := os.OpenFile(out, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, os.FileMode(hdr.Mode))
			if err != nil {
				return err
			}
			if _, err := io.Copy(f, tr); err != nil {
				f.Close()
				return err
			}
			f.Close()
		case tar.TypeSymlink:
			_ = os.Symlink(hdr.Linkname, out)
		}
	}
}
