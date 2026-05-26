// Package embed handles the self-extracting fat-binary feature.
//
// `perch --build -f commands.perch -o myapp` copies the running perch
// executable, marshals the loaded domain.Program to JSON, and appends a
// footer of the form:
//
//   <original executor bytes>
//   <json bytes>
//   <8 bytes: big-endian uint64 length of json>
//   <8 bytes: magic = "PRCHEMB1">
//
// At startup, perch reads the last 16 bytes of os.Executable() — if the
// magic matches, it loads the embedded JSON instead of looking for a
// .perch file on disk.
package embed

import (
	"encoding/binary"
	"encoding/json"
	"fmt"
	"io"
	"os"

	"github.com/luowensheng/perch/domain"
)

// Magic is the 8-byte sentinel marking an embedded program at EOF.
var Magic = [8]byte{'P', 'R', 'C', 'H', 'E', 'M', 'B', '1'}

// Embed writes a copy of the source binary (typically the running perch)
// to outPath and appends the program JSON + footer.
func Embed(sourceBinary string, p *domain.Program, outPath string) error {
	srcBytes, err := os.ReadFile(sourceBinary)
	if err != nil {
		return fmt.Errorf("read source binary: %w", err)
	}
	// If the source already has an embedded program, strip it first so
	// rebuilds don't accumulate layers.
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
	var lenBuf [8]byte
	binary.BigEndian.PutUint64(lenBuf[:], uint64(len(progJSON)))
	if _, err := out.Write(lenBuf[:]); err != nil {
		return err
	}
	if _, err := out.Write(Magic[:]); err != nil {
		return err
	}
	return nil
}

// Load reads the embedded program from the current executable if present.
// Returns (nil, false, nil) when nothing is embedded.
func Load() (*domain.Program, bool, error) {
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

	// Read the footer (last 16 bytes).
	if _, err := f.Seek(info.Size()-16, io.SeekStart); err != nil {
		return nil, false, nil
	}
	var footer [16]byte
	if _, err := io.ReadFull(f, footer[:]); err != nil {
		return nil, false, nil
	}
	if footer[8] != Magic[0] || footer[9] != Magic[1] || footer[10] != Magic[2] ||
		footer[11] != Magic[3] || footer[12] != Magic[4] || footer[13] != Magic[5] ||
		footer[14] != Magic[6] || footer[15] != Magic[7] {
		return nil, false, nil
	}
	jsonLen := binary.BigEndian.Uint64(footer[:8])
	if int64(jsonLen) > info.Size()-16 {
		return nil, false, fmt.Errorf("embedded length %d exceeds file size", jsonLen)
	}

	if _, err := f.Seek(info.Size()-16-int64(jsonLen), io.SeekStart); err != nil {
		return nil, false, err
	}
	buf := make([]byte, jsonLen)
	if _, err := io.ReadFull(f, buf); err != nil {
		return nil, false, err
	}
	var p domain.Program
	if err := json.Unmarshal(buf, &p); err != nil {
		return nil, false, fmt.Errorf("decode embedded program: %w", err)
	}
	return &p, true, nil
}

// stripExisting removes a footer if present (idempotent).
func stripExisting(b []byte) []byte {
	if len(b) < 16 {
		return b
	}
	tail := b[len(b)-16:]
	for i := 0; i < 8; i++ {
		if tail[8+i] != Magic[i] {
			return b
		}
	}
	jsonLen := binary.BigEndian.Uint64(tail[:8])
	if int64(jsonLen)+16 > int64(len(b)) {
		return b
	}
	return b[:int64(len(b))-int64(jsonLen)-16]
}
