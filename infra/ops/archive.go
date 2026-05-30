package ops

import (
	"archive/tar"
	"archive/zip"
	"compress/gzip"
	"io"
	"os"
	"path/filepath"
	"strings"

	"github.com/olivierdevelops/perch/infra/interpreter"
)

func registerArchive(m map[string]interpreter.Handler) {
	m["tar_create"] = opTarCreate
	m["tar_extract"] = opTarExtract
	m["zip_create"] = opZipCreate
	m["zip_extract"] = opZipExtract
}

// tar_create SRC_DIR DST.tar.gz — creates a gzipped tarball of SRC_DIR.
func opTarCreate(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	src := resolve(argString(args, "src"), b)
	dst := resolve(argString(args, "dst"), b)
	out, err := os.Create(dst)
	if err != nil {
		return nil, err
	}
	defer out.Close()
	gz := gzip.NewWriter(out)
	defer gz.Close()
	tw := tar.NewWriter(gz)
	defer tw.Close()

	return nil, filepath.Walk(src, func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return err
		}
		rel, _ := filepath.Rel(src, path)
		if rel == "." {
			return nil
		}
		hdr, err := tar.FileInfoHeader(info, "")
		if err != nil {
			return err
		}
		hdr.Name = rel
		if err := tw.WriteHeader(hdr); err != nil {
			return err
		}
		if info.IsDir() {
			return nil
		}
		f, err := os.Open(path)
		if err != nil {
			return err
		}
		defer f.Close()
		_, err = io.Copy(tw, f)
		return err
	})
}

func opTarExtract(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	src := resolve(argString(args, "src"), b)
	dst := resolve(argString(args, "dst"), b)
	in, err := os.Open(src)
	if err != nil {
		return nil, err
	}
	defer in.Close()
	gz, err := gzip.NewReader(in)
	if err != nil {
		return nil, err
	}
	defer gz.Close()
	tr := tar.NewReader(gz)
	for {
		hdr, err := tr.Next()
		if err == io.EOF {
			return nil, nil
		}
		if err != nil {
			return nil, err
		}
		out := filepath.Join(dst, hdr.Name)
		if strings.Contains(hdr.Name, "..") {
			continue
		}
		switch hdr.Typeflag {
		case tar.TypeDir:
			if err := os.MkdirAll(out, os.FileMode(hdr.Mode)); err != nil {
				return nil, err
			}
		case tar.TypeReg:
			if err := os.MkdirAll(filepath.Dir(out), 0755); err != nil {
				return nil, err
			}
			f, err := os.OpenFile(out, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, os.FileMode(hdr.Mode))
			if err != nil {
				return nil, err
			}
			if _, err := io.Copy(f, tr); err != nil {
				f.Close()
				return nil, err
			}
			f.Close()
		}
	}
}

func opZipCreate(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	src := resolve(argString(args, "src"), b)
	dst := resolve(argString(args, "dst"), b)
	out, err := os.Create(dst)
	if err != nil {
		return nil, err
	}
	defer out.Close()
	zw := zip.NewWriter(out)
	defer zw.Close()
	return nil, filepath.Walk(src, func(path string, info os.FileInfo, err error) error {
		if err != nil || info.IsDir() {
			return err
		}
		rel, _ := filepath.Rel(src, path)
		f, err := zw.Create(rel)
		if err != nil {
			return err
		}
		in, err := os.Open(path)
		if err != nil {
			return err
		}
		defer in.Close()
		_, err = io.Copy(f, in)
		return err
	})
}

func opZipExtract(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	src := resolve(argString(args, "src"), b)
	dst := resolve(argString(args, "dst"), b)
	zr, err := zip.OpenReader(src)
	if err != nil {
		return nil, err
	}
	defer zr.Close()
	for _, f := range zr.File {
		out := filepath.Join(dst, f.Name)
		if strings.Contains(f.Name, "..") {
			continue
		}
		if f.FileInfo().IsDir() {
			if err := os.MkdirAll(out, 0755); err != nil {
				return nil, err
			}
			continue
		}
		if err := os.MkdirAll(filepath.Dir(out), 0755); err != nil {
			return nil, err
		}
		rc, err := f.Open()
		if err != nil {
			return nil, err
		}
		w, err := os.OpenFile(out, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, f.Mode())
		if err != nil {
			rc.Close()
			return nil, err
		}
		if _, err := io.Copy(w, rc); err != nil {
			rc.Close()
			w.Close()
			return nil, err
		}
		rc.Close()
		w.Close()
	}
	return nil, nil
}
