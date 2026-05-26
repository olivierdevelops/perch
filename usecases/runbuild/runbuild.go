// Package runbuild produces a portable, self-extracting perch binary
// with the loaded program embedded, and optionally a tarball of an
// arbitrary file tree (`--include <path>`) so the resulting binary
// can install a Python / JS / any non-native project anywhere.
package runbuild

import (
	"archive/tar"
	"bytes"
	"compress/gzip"
	"flag"
	"fmt"
	"io"
	"os"
	"path/filepath"

	"github.com/luowensheng/perch/domain"
)

type UseCase interface {
	Execute(configPath string, args []string) error
}

type LoadFn func(path string) (*domain.Program, error)
type EmbedFn func(sourceBinary string, p *domain.Program, archive []byte, outPath string) error

type Impl struct {
	Load  LoadFn
	Embed EmbedFn
}

func (i *Impl) Execute(configPath string, args []string) error {
	fs := flag.NewFlagSet("build", flag.ExitOnError)
	out := fs.String("o", "", "Output binary path (default: program name from commands.perch)")
	include := fs.String("include", "", "Path (file or directory) to embed as a tarball inside the binary")
	if err := fs.Parse(args); err != nil {
		return err
	}

	p, err := i.Load(configPath)
	if err != nil {
		return err
	}

	outPath := *out
	if outPath == "" {
		outPath = p.Name
		if outPath == "" {
			outPath = "perch-app"
		}
	}

	self, err := os.Executable()
	if err != nil {
		return fmt.Errorf("locate self: %w", err)
	}

	var archive []byte
	if *include != "" {
		fmt.Printf("Bundling %s …\n", *include)
		archive, err = tarballPath(*include)
		if err != nil {
			return fmt.Errorf("tar %q: %w", *include, err)
		}
		fmt.Printf("✓ embedded %d bytes\n", len(archive))
	}

	if err := i.Embed(self, p, archive, outPath); err != nil {
		return err
	}

	abs, _ := filepath.Abs(outPath)
	fmt.Println("Built binary:", abs)
	return nil
}

// tarballPath produces a gzipped tar of `root`. If `root` is a file
// the archive contains exactly that file; if it's a directory, every
// file under it is archived (skipping .git / node_modules / __pycache__
// / .venv / venv / .tox / dist / .cache / .DS_Store).
func tarballPath(root string) ([]byte, error) {
	info, err := os.Stat(root)
	if err != nil {
		return nil, err
	}
	var buf bytes.Buffer
	gz := gzip.NewWriter(&buf)
	tw := tar.NewWriter(gz)

	if !info.IsDir() {
		if err := tarAddFile(tw, root, filepath.Base(root), info); err != nil {
			return nil, err
		}
	} else {
		walkErr := filepath.Walk(root, func(path string, fi os.FileInfo, err error) error {
			if err != nil {
				return err
			}
			rel, err := filepath.Rel(root, path)
			if err != nil {
				return err
			}
			if rel == "." {
				return nil
			}
			if fi.IsDir() {
				switch fi.Name() {
				case ".git", "node_modules", "__pycache__", ".venv", "venv", ".tox", "dist", ".cache":
					return filepath.SkipDir
				}
			}
			if fi.Name() == ".DS_Store" {
				return nil
			}
			return tarAddFile(tw, path, rel, fi)
		})
		if walkErr != nil {
			return nil, walkErr
		}
	}

	if err := tw.Close(); err != nil {
		return nil, err
	}
	if err := gz.Close(); err != nil {
		return nil, err
	}
	return buf.Bytes(), nil
}

func tarAddFile(tw *tar.Writer, src, name string, fi os.FileInfo) error {
	hdr, err := tar.FileInfoHeader(fi, "")
	if err != nil {
		return err
	}
	hdr.Name = filepath.ToSlash(name)
	if err := tw.WriteHeader(hdr); err != nil {
		return err
	}
	if fi.IsDir() {
		return nil
	}
	f, err := os.Open(src)
	if err != nil {
		return err
	}
	defer f.Close()
	_, err = io.Copy(tw, f)
	return err
}
