// policy-check.wasm — pre-deploy policy enforcement.
//
// Reads YAML / JSON config files from /ro/configs/ and verifies they
// pass a set of operational policies. Returns exit 0 on clean, 1 on
// any violation. Designed to drop into CI as a final gate:
//
//   perch -f predeploy.perch enforce
//
// Replaces the typical "60-line bash + grep + yq + jq" pre-deploy
// script with a deterministic, sha256-pinnable WASM artifact.
//
// Policies implemented (extend by adding to the Policies map):
//   1. registry-allowlist     — container images must come from
//                                approved registries
//   2. resource-limits        — pods must declare CPU + memory limits
//   3. no-latest-tag          — image tags can't be "latest"
//   4. no-privileged          — securityContext.privileged must not be true
//   5. required-labels        — pods must carry the policy-declared
//                                labels (team, env)
//
// Build:
//   GOOS=wasip1 GOARCH=wasm go build -o policy-check.wasm .
package main

import (
	"encoding/json"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"sort"
	"strings"

	"gopkg.in/yaml.v3"
)

// Violation is one specific policy failure.
type Violation struct {
	File    string `json:"file"`
	Policy  string `json:"policy"`
	Where   string `json:"where"`
	Got     string `json:"got"`
	Want    string `json:"want"`
}

// allowedRegistries is the configurable allowlist. In a real
// deployment this would be passed in via wasm_arg or read from a
// mounted config file; we keep it inline for the demo.
var allowedRegistries = []string{
	"ghcr.io/",
	"registry.company.internal/",
	"docker.io/library/",
}

var requiredLabels = []string{"team", "env"}

func main() {
	if len(os.Args) < 2 {
		fmt.Fprintln(os.Stderr, "usage: policy-check <dir>")
		os.Exit(2)
	}
	root := os.Args[1]

	var violations []Violation
	err := filepath.WalkDir(root, func(path string, d fs.DirEntry, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		if d.IsDir() {
			return nil
		}
		if !strings.HasSuffix(path, ".yaml") && !strings.HasSuffix(path, ".yml") && !strings.HasSuffix(path, ".json") {
			return nil
		}
		data, err := os.ReadFile(path)
		if err != nil {
			return err
		}
		var docs []map[string]any
		if strings.HasSuffix(path, ".json") {
			var v map[string]any
			if err := json.Unmarshal(data, &v); err != nil {
				return fmt.Errorf("%s: %w", path, err)
			}
			docs = []map[string]any{v}
		} else {
			// YAML files may contain multiple --- separated docs.
			dec := yaml.NewDecoder(strings.NewReader(string(data)))
			for {
				var v map[string]any
				if err := dec.Decode(&v); err != nil {
					if err.Error() == "EOF" {
						break
					}
					return fmt.Errorf("%s: %w", path, err)
				}
				if v != nil {
					docs = append(docs, v)
				}
			}
		}
		for i, doc := range docs {
			label := path
			if len(docs) > 1 {
				label = fmt.Sprintf("%s[doc#%d]", path, i)
			}
			violations = append(violations, runPolicies(label, doc)...)
		}
		return nil
	})
	if err != nil {
		fmt.Fprintln(os.Stderr, "walk:", err)
		os.Exit(2)
	}

	if len(violations) == 0 {
		fmt.Printf("✓ %s: policies passed\n", root)
		os.Exit(0)
	}
	sort.Slice(violations, func(i, j int) bool {
		if violations[i].File != violations[j].File {
			return violations[i].File < violations[j].File
		}
		return violations[i].Policy < violations[j].Policy
	})
	for _, v := range violations {
		fmt.Fprintf(os.Stderr, "✗ [%s] %s\n  at  %s\n  got %s\n  want %s\n\n",
			v.Policy, v.File, v.Where, v.Got, v.Want)
	}
	fmt.Fprintf(os.Stderr, "%d violation(s) across %d file(s)\n", len(violations), countFiles(violations))
	os.Exit(1)
}

func runPolicies(file string, doc map[string]any) []Violation {
	var out []Violation
	out = append(out, policyRegistryAllowlist(file, doc)...)
	out = append(out, policyResourceLimits(file, doc)...)
	out = append(out, policyNoLatestTag(file, doc)...)
	out = append(out, policyNoPrivileged(file, doc)...)
	out = append(out, policyRequiredLabels(file, doc)...)
	return out
}

// containers walks the standard K8s pod-spec shape to find every
// container/initContainer object. PodSpec may live at .spec (Pod) or
// .spec.template.spec (Deployment/Job/etc.) — check both.
func containers(doc map[string]any) []map[string]any {
	var out []map[string]any
	collect := func(spec map[string]any) {
		if spec == nil {
			return
		}
		for _, key := range []string{"containers", "initContainers"} {
			if list, ok := spec[key].([]any); ok {
				for _, c := range list {
					if cm, ok := c.(map[string]any); ok {
						out = append(out, cm)
					}
				}
			}
		}
	}
	if m, ok := nested(doc, "spec").(map[string]any); ok {
		collect(m)
	}
	if m, ok := nested(doc, "spec", "template", "spec").(map[string]any); ok {
		collect(m)
	}
	return out
}

func policyRegistryAllowlist(file string, doc map[string]any) []Violation {
	var out []Violation
	for _, c := range containers(doc) {
		img, _ := c["image"].(string)
		if img == "" {
			continue
		}
		ok := false
		for _, prefix := range allowedRegistries {
			if strings.HasPrefix(img, prefix) {
				ok = true
				break
			}
		}
		if !ok {
			out = append(out, Violation{
				File:   file,
				Policy: "registry-allowlist",
				Where:  fmt.Sprintf("container %q", c["name"]),
				Got:    img,
				Want:   "one of: " + strings.Join(allowedRegistries, ", "),
			})
		}
	}
	return out
}

func policyResourceLimits(file string, doc map[string]any) []Violation {
	var out []Violation
	for _, c := range containers(doc) {
		res, _ := c["resources"].(map[string]any)
		if res == nil {
			out = append(out, Violation{
				File:   file,
				Policy: "resource-limits",
				Where:  fmt.Sprintf("container %q", c["name"]),
				Got:    "no resources block",
				Want:   "resources.limits.cpu + resources.limits.memory",
			})
			continue
		}
		limits, _ := res["limits"].(map[string]any)
		if limits == nil {
			out = append(out, Violation{
				File:   file,
				Policy: "resource-limits",
				Where:  fmt.Sprintf("container %q", c["name"]),
				Got:    "no limits block",
				Want:   "limits.cpu + limits.memory",
			})
			continue
		}
		for _, key := range []string{"cpu", "memory"} {
			if _, ok := limits[key]; !ok {
				out = append(out, Violation{
					File:   file,
					Policy: "resource-limits",
					Where:  fmt.Sprintf("container %q", c["name"]),
					Got:    "missing limits." + key,
					Want:   "a value (e.g. " + key + ": 500m)",
				})
			}
		}
	}
	return out
}

func policyNoLatestTag(file string, doc map[string]any) []Violation {
	var out []Violation
	for _, c := range containers(doc) {
		img, _ := c["image"].(string)
		if img == "" {
			continue
		}
		if strings.HasSuffix(img, ":latest") || !strings.Contains(img, ":") {
			out = append(out, Violation{
				File:   file,
				Policy: "no-latest-tag",
				Where:  fmt.Sprintf("container %q", c["name"]),
				Got:    img,
				Want:   "a pinned tag (e.g. registry/image:v1.2.3)",
			})
		}
	}
	return out
}

func policyNoPrivileged(file string, doc map[string]any) []Violation {
	var out []Violation
	for _, c := range containers(doc) {
		sec, _ := c["securityContext"].(map[string]any)
		if sec == nil {
			continue
		}
		if priv, _ := sec["privileged"].(bool); priv {
			out = append(out, Violation{
				File:   file,
				Policy: "no-privileged",
				Where:  fmt.Sprintf("container %q.securityContext.privileged", c["name"]),
				Got:    "true",
				Want:   "false or omitted",
			})
		}
	}
	return out
}

func policyRequiredLabels(file string, doc map[string]any) []Violation {
	var out []Violation
	labels, _ := nested(doc, "metadata", "labels").(map[string]any)
	for _, req := range requiredLabels {
		if labels == nil || labels[req] == nil {
			out = append(out, Violation{
				File:   file,
				Policy: "required-labels",
				Where:  "metadata.labels",
				Got:    "missing",
				Want:   "label " + req,
			})
		}
	}
	return out
}

// nested follows a path of keys into a nested map structure.
// Returns nil at the first non-map step.
func nested(m map[string]any, keys ...string) any {
	var cur any = m
	for _, k := range keys {
		mm, ok := cur.(map[string]any)
		if !ok {
			return nil
		}
		cur = mm[k]
	}
	return cur
}

func countFiles(vs []Violation) int {
	seen := map[string]bool{}
	for _, v := range vs {
		seen[v.File] = true
	}
	return len(seen)
}
