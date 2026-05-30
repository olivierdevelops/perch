package ops

import (
	"crypto/md5"
	"crypto/sha1"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"hash"
	"hash/crc32"
	"io"
	"os"

	"github.com/olivierdevelops/perch/infra/interpreter"
)

func registerHash(m map[string]interpreter.Handler) {
	m["md5"] = stringHash(md5.New)
	m["sha1"] = stringHash(sha1.New)
	m["sha256"] = stringHash(sha256.New)
	m["md5_file"] = fileHash(md5.New)
	m["sha1_file"] = fileHash(sha1.New)
	m["sha256_file"] = fileHash(sha256.New)
	m["crc32"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		v := crc32.ChecksumIEEE([]byte(argString(args, "value", "_0")))
		return fmt.Sprintf("%08x", v), nil
	}
	// verify_sha256 PATH HASH → bool. Common during install for
	// integrity-checking downloaded archives.
	m["verify_sha256"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		path := resolve(argString(args, "path", "_0"), b)
		expected := argString(args, "hash", "_1")
		f, err := os.Open(path)
		if err != nil {
			return false, nil
		}
		defer f.Close()
		h := sha256.New()
		if _, err := io.Copy(h, f); err != nil {
			return false, err
		}
		got := hex.EncodeToString(h.Sum(nil))
		return got == expected, nil
	}
}

func stringHash(new func() hash.Hash) interpreter.Handler {
	return func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		h := new()
		h.Write([]byte(argString(args, "value", "_0")))
		return hex.EncodeToString(h.Sum(nil)), nil
	}
}

func fileHash(new func() hash.Hash) interpreter.Handler {
	return func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		f, err := os.Open(resolve(argString(args, "path", "_0"), b))
		if err != nil {
			return "", err
		}
		defer f.Close()
		h := new()
		if _, err := io.Copy(h, f); err != nil {
			return "", err
		}
		return hex.EncodeToString(h.Sum(nil)), nil
	}
}
