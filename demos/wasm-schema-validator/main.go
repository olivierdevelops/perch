// schema-validator.wasm — lightweight JSON schema validator.
//
// Usage: validator <schema.json> <document.json>
// Exit codes:
//   0  document matches the schema
//   1  document violates the schema (errors on stderr)
//   2  bad invocation / schema / document parse error
//
// Build (Go 1.21+, no external deps):
//   GOOS=wasip1 GOARCH=wasm go build -o schema-validator.wasm .
//
// Why a hand-rolled validator (vs pulling in a heavy library):
//   - Stdlib-only → reliable WASI Preview 1 compatibility
//   - 200 lines → fully auditable, sha256-pinnable
//   - Covers the 80% of validation real configs need:
//     type, required fields, enum, pattern, min/max, additionalProperties
package main

import (
	"encoding/json"
	"fmt"
	"os"
	"regexp"
	"sort"
	"strings"
)

// schema is the supported subset of JSON Schema.
type schema struct {
	Type                 string             `json:"type,omitempty"`
	Properties           map[string]*schema `json:"properties,omitempty"`
	Required             []string           `json:"required,omitempty"`
	Enum                 []any              `json:"enum,omitempty"`
	Pattern              string             `json:"pattern,omitempty"`
	Minimum              *float64           `json:"minimum,omitempty"`
	Maximum              *float64           `json:"maximum,omitempty"`
	MinLength            *int               `json:"minLength,omitempty"`
	MaxLength            *int               `json:"maxLength,omitempty"`
	Items                *schema            `json:"items,omitempty"`
	AdditionalProperties *bool              `json:"additionalProperties,omitempty"`
	Description          string             `json:"description,omitempty"`
}

func main() {
	if len(os.Args) < 3 {
		fmt.Fprintln(os.Stderr, "usage: validator <schema.json> <document.json>")
		os.Exit(2)
	}
	sch, err := loadSchema(os.Args[1])
	if err != nil {
		fmt.Fprintln(os.Stderr, "load schema:", err)
		os.Exit(2)
	}
	doc, err := loadDoc(os.Args[2])
	if err != nil {
		fmt.Fprintln(os.Stderr, "load doc:", err)
		os.Exit(2)
	}
	errs := validate(sch, doc, "")
	if len(errs) > 0 {
		sort.Strings(errs)
		for _, e := range errs {
			fmt.Fprintln(os.Stderr, "✗", e)
		}
		fmt.Fprintf(os.Stderr, "%s: %d error(s)\n", os.Args[2], len(errs))
		os.Exit(1)
	}
	fmt.Printf("%s: ✓ valid\n", os.Args[2])
}

func loadSchema(path string) (*schema, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	var s schema
	if err := json.Unmarshal(data, &s); err != nil {
		return nil, fmt.Errorf("schema parse: %w", err)
	}
	return &s, nil
}

func loadDoc(path string) (any, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	var v any
	if err := json.Unmarshal(data, &v); err != nil {
		return nil, fmt.Errorf("document parse: %w", err)
	}
	return v, nil
}

// validate walks the schema and returns every violation found.
// Errors are paths-into-the-document-prefixed for readability:
//
//	user.address.zip: must be string, got number
func validate(s *schema, v any, path string) []string {
	var errs []string
	add := func(msg string) {
		where := path
		if where == "" {
			where = "<root>"
		}
		errs = append(errs, where+": "+msg)
	}

	if s.Type != "" {
		if !checkType(s.Type, v) {
			add(fmt.Sprintf("must be %s, got %s", s.Type, jsonTypeOf(v)))
			return errs // further checks would just be noise
		}
	}

	if len(s.Enum) > 0 {
		ok := false
		for _, e := range s.Enum {
			if jsonDeepEqual(e, v) {
				ok = true
				break
			}
		}
		if !ok {
			add(fmt.Sprintf("must be one of %v, got %v", s.Enum, v))
		}
	}

	switch tv := v.(type) {
	case string:
		if s.Pattern != "" {
			re, err := regexp.Compile(s.Pattern)
			if err != nil {
				add(fmt.Sprintf("schema pattern invalid: %v", err))
			} else if !re.MatchString(tv) {
				add(fmt.Sprintf("must match /%s/, got %q", s.Pattern, tv))
			}
		}
		if s.MinLength != nil && len(tv) < *s.MinLength {
			add(fmt.Sprintf("min length %d, got %d", *s.MinLength, len(tv)))
		}
		if s.MaxLength != nil && len(tv) > *s.MaxLength {
			add(fmt.Sprintf("max length %d, got %d", *s.MaxLength, len(tv)))
		}

	case float64:
		if s.Minimum != nil && tv < *s.Minimum {
			add(fmt.Sprintf("must be ≥ %v, got %v", *s.Minimum, tv))
		}
		if s.Maximum != nil && tv > *s.Maximum {
			add(fmt.Sprintf("must be ≤ %v, got %v", *s.Maximum, tv))
		}

	case []any:
		if s.Items != nil {
			for i, item := range tv {
				errs = append(errs, validate(s.Items, item, fmt.Sprintf("%s[%d]", path, i))...)
			}
		}

	case map[string]any:
		// Required fields.
		for _, r := range s.Required {
			if _, ok := tv[r]; !ok {
				add(fmt.Sprintf("missing required field %q", r))
			}
		}
		// Per-property schemas.
		for k, sub := range s.Properties {
			if val, ok := tv[k]; ok {
				p := k
				if path != "" {
					p = path + "." + k
				}
				errs = append(errs, validate(sub, val, p)...)
			}
		}
		// Unknown fields when additionalProperties: false.
		if s.AdditionalProperties != nil && !*s.AdditionalProperties {
			for k := range tv {
				if _, declared := s.Properties[k]; !declared {
					add(fmt.Sprintf("unknown field %q (additionalProperties: false)", k))
				}
			}
		}
	}

	return errs
}

func checkType(expected string, v any) bool {
	switch expected {
	case "string":
		_, ok := v.(string)
		return ok
	case "number":
		_, ok := v.(float64)
		return ok
	case "integer":
		f, ok := v.(float64)
		return ok && f == float64(int64(f))
	case "boolean":
		_, ok := v.(bool)
		return ok
	case "array":
		_, ok := v.([]any)
		return ok
	case "object":
		_, ok := v.(map[string]any)
		return ok
	case "null":
		return v == nil
	}
	return true // unknown type → pass
}

func jsonTypeOf(v any) string {
	switch v.(type) {
	case string:
		return "string"
	case float64:
		return "number"
	case bool:
		return "boolean"
	case []any:
		return "array"
	case map[string]any:
		return "object"
	case nil:
		return "null"
	}
	return strings.TrimPrefix(fmt.Sprintf("%T", v), "*")
}

func jsonDeepEqual(a, b any) bool {
	ab, _ := json.Marshal(a)
	bb, _ := json.Marshal(b)
	return string(ab) == string(bb)
}
