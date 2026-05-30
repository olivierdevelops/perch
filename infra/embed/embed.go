// Package embed handles the self-extracting fat-binary feature.
//
// `perch --build -f commands.perch -o myapp` copies the running perch
// executable, marshals the loaded domain.Program to JSON, optionally
// appends an arbitrary file-tree as a gzipped tarball ("the bundle"),
// and writes a 24-byte footer:
//
//   <original executor bytes>
//   <json bytes>
//   <archive bytes>                 (may be empty)
//   <8 bytes: big-endian uint64 archive length>
//   <8 bytes: big-endian uint64 json length>
//   <8 bytes: magic = "PRCHEMB2">
//
// At startup, perch reads the last 24 bytes of os.Executable() — if the
// magic matches, it loads the embedded JSON (and remembers the archive
// for the bundle_dir / bundle_hash / bundle_extract ops).
//
// V1 binaries (footer "PRCHEMB1", 16 bytes, no archive) still load —
// existing fat binaries built with older perch keep working.
package embed

import (
	"crypto/sha256"
	"encoding/binary"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"os"

	"github.com/olivierdevelops/perch/domain"
)

// MagicV2 is the current footer sentinel — 8 ASCII bytes at EOF.
var MagicV2 = [8]byte{'P', 'R', 'C', 'H', 'E', 'M', 'B', '2'}

// MagicV1 is the legacy footer (no archive section). Still loaded if
// encountered so old binaries don't break.
var MagicV1 = [8]byte{'P', 'R', 'C', 'H', 'E', 'M', 'B', '1'}

// Bundle is what Load returns: the parsed program plus an optional
// payload archive (gzipped tarball) embedded by `--build --include`.
type Bundle struct {
	Program *domain.Program
	Archive []byte // may be nil
	// ArchiveHash is the SHA-256 hex of the embedded archive bytes
	// (empty string when Archive is nil). Useful for content-addressable
	// install paths.
	ArchiveHash string
}

// Embed writes a copy of the source binary to outPath, appending the
// program JSON, the optional archive, and a V2 footer.
func Embed(sourceBinary string, p *domain.Program, archive []byte, outPath string) error {
	srcBytes, err := os.ReadFile(sourceBinary)
	if err != nil {
		return fmt.Errorf("read source binary: %w", err)
	}
	// Strip any existing footer (idempotent rebuilds).
	srcBytes = stripExisting(srcBytes)

	progJSON, err := json.Marshal(p)
	if err != nil {
		return fmt.Errorf("marshal program: %w", err)
	}

	out, err := os.OpenFile(outPath, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, 0755)
	if err != nil {
		return fmt.Errorf("open output: %w", err)
	}
	defer out.Close()

	if _, err := out.Write(srcBytes); err != nil {
		return err
	}
	if _, err := out.Write(progJSON); err != nil {
		return err
	}
	if len(archive) > 0 {
		if _, err := out.Write(archive); err != nil {
			return err
		}
	}
	var aLen, jLen [8]byte
	binary.BigEndian.PutUint64(aLen[:], uint64(len(archive)))
	binary.BigEndian.PutUint64(jLen[:], uint64(len(progJSON)))
	if _, err := out.Write(aLen[:]); err != nil {
		return err
	}
	if _, err := out.Write(jLen[:]); err != nil {
		return err
	}
	if _, err := out.Write(MagicV2[:]); err != nil {
		return err
	}
	return nil
}

// Load reads the embedded bundle from the current executable. Returns
// (nil, false, nil) when nothing is embedded.
func Load() (*Bundle, bool, error) {
	exe, err := os.Executable()
	if err != nil {
		return nil, false, nil
	}
	f, err := os.Open(exe)
	if err != nil {
		return nil, false, nil
	}
	defer f.Close()

	info, err := f.Stat()
	if err != nil {
		return nil, false, nil
	}
	if info.Size() < 16 {
		return nil, false, nil
	}

	// Peek at the last 8 bytes for the magic.
	if _, err := f.Seek(info.Size()-8, io.SeekStart); err != nil {
		return nil, false, nil
	}
	var magic [8]byte
	if _, err := io.ReadFull(f, magic[:]); err != nil {
		return nil, false, nil
	}

	switch magic {
	case MagicV2:
		return loadV2(f, info.Size())
	case MagicV1:
		return loadV1(f, info.Size())
	}
	return nil, false, nil
}

func loadV2(f *os.File, size int64) (*Bundle, bool, error) {
	// Footer: <a_len 8><j_len 8><magic 8> = 24 bytes total.
	if size < 24 {
		return nil, false, nil
	}
	if _, err := f.Seek(size-24, io.SeekStart); err != nil {
		return nil, false, err
	}
	var aLenBuf, jLenBuf [8]byte
	if _, err := io.ReadFull(f, aLenBuf[:]); err != nil {
		return nil, false, err
	}
	if _, err := io.ReadFull(f, jLenBuf[:]); err != nil {
		return nil, false, err
	}
	aLen := int64(binary.BigEndian.Uint64(aLenBuf[:]))
	jLen := int64(binary.BigEndian.Uint64(jLenBuf[:]))
	total := aLen + jLen + 24
	if total > size {
		return nil, false, fmt.Errorf("embedded length %d exceeds file size", total)
	}

	// Read program JSON.
	if _, err := f.Seek(size-24-aLen-jLen, io.SeekStart); err != nil {
		return nil, false, err
	}
	progBuf := make([]byte, jLen)
	if _, err := io.ReadFull(f, progBuf); err != nil {
		return nil, false, err
	}
	var p domain.Program
	if err := json.Unmarshal(progBuf, &p); err != nil {
		return nil, false, fmt.Errorf("decode embedded program: %w", err)
	}

	var arch []byte
	var archHash string
	if aLen > 0 {
		arch = make([]byte, aLen)
		if _, err := io.ReadFull(f, arch); err != nil {
			return nil, false, err
		}
		h := sha256.Sum256(arch)
		archHash = hex.EncodeToString(h[:])
	}
	return &Bundle{Program: &p, Archive: arch, ArchiveHash: archHash}, true, nil
}

func loadV1(f *os.File, size int64) (*Bundle, bool, error) {
	// V1 footer: <j_len 8><magic 8> = 16 bytes total.
	if size < 16 {
		return nil, false, nil
	}
	if _, err := f.Seek(size-16, io.SeekStart); err != nil {
		return nil, false, err
	}
	var jLenBuf [8]byte
	if _, err := io.ReadFull(f, jLenBuf[:]); err != nil {
		return nil, false, err
	}
	jLen := int64(binary.BigEndian.Uint64(jLenBuf[:]))
	if jLen+16 > size {
		return nil, false, fmt.Errorf("embedded length %d exceeds file size", jLen)
	}
	if _, err := f.Seek(size-16-jLen, io.SeekStart); err != nil {
		return nil, false, err
	}
	progBuf := make([]byte, jLen)
	if _, err := io.ReadFull(f, progBuf); err != nil {
		return nil, false, err
	}
	var p domain.Program
	if err := json.Unmarshal(progBuf, &p); err != nil {
		return nil, false, fmt.Errorf("decode embedded program: %w", err)
	}
	return &Bundle{Program: &p}, true, nil
}

// stripExisting removes a V1 or V2 footer if present (idempotent rebuilds).
func stripExisting(b []byte) []byte {
	if len(b) < 8 {
		return b
	}
	tail := b[len(b)-8:]
	// V2?
	if matches(tail, MagicV2[:]) && len(b) >= 24 {
		aLen := int64(binary.BigEndian.Uint64(b[len(b)-24:]))
		jLen := int64(binary.BigEndian.Uint64(b[len(b)-16:]))
		total := aLen + jLen + 24
		if total <= int64(len(b)) {
			return b[:int64(len(b))-total]
		}
		return b
	}
	// V1?
	if matches(tail, MagicV1[:]) && len(b) >= 16 {
		jLen := int64(binary.BigEndian.Uint64(b[len(b)-16:]))
		total := jLen + 16
		if total <= int64(len(b)) {
			return b[:int64(len(b))-total]
		}
	}
	return b
}

func matches(a, b []byte) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}
