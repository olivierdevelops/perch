// HTTP ops with hardened defaults against the classic redirect / SSRF
// attack surface. Three protections, all on by default:
//
//   1. **No requests or redirects to private / loopback / link-local IPs.**
//      Closes the AWS-metadata-service SSRF (169.254.169.254), the
//      localhost-pivot (`shell` ops blocked but http_get reaches your
//      local Redis anyway), and the RFC 1918 internal-network pivot.
//   2. **No https → http redirect downgrade.** A `https://x` that 30x's
//      to `http://x` is refused.
//   3. **Cap of 5 redirect hops** (default; configurable).
//
// Each is opt-OUT via a CLI flag — sane defaults, escape hatch when
// the user genuinely needs e.g. to hit a localhost service.
//
// The validation runs on the INITIAL request URL too (not just on
// redirects), so http_get "http://169.254.169.254/" is refused before
// any packet leaves the host.
package ops

import (
	"fmt"
	"io"
	"net"
	"net/http"
	"net/url"
	"os"
	"strings"
	"time"

	"github.com/luowensheng/perch/infra/interpreter"
)

func registerHTTP(m map[string]interpreter.Handler) {
	m["http_get"] = opHTTPGet
	m["http_post"] = opHTTPMethod("POST")
	m["http_put"] = opHTTPMethod("PUT")
	m["http_delete"] = opHTTPMethod("DELETE")
	m["download"] = opDownload
}

// httpPolicy reads the active policy from the interpreter, falling
// back to the secure defaults when the interpreter doesn't carry one.
func httpPolicy(i *interpreter.Interpreter) interpreter.HTTPPolicy {
	if i.HTTPPolicy != nil {
		return *i.HTTPPolicy
	}
	return interpreter.HTTPPolicy{
		MaxRedirects:         5,
		AllowPrivateIPs:      false,
		AllowSchemeDowngrade: false,
	}
}

// newHTTPClient builds an http.Client whose CheckRedirect enforces the
// policy. Each new HTTP op call builds a fresh client (cheap — Go's
// http.Client is value-cheap).
func newHTTPClient(p interpreter.HTTPPolicy) *http.Client {
	return &http.Client{
		Timeout: 30 * time.Second,
		CheckRedirect: func(req *http.Request, via []*http.Request) error {
			if p.MaxRedirects == 0 {
				return fmt.Errorf("redirect refused by --no-redirects (target: %s)", req.URL)
			}
			if len(via) >= p.MaxRedirects {
				return fmt.Errorf("max %d redirects exceeded (target: %s)", p.MaxRedirects, req.URL)
			}
			if !p.AllowSchemeDowngrade && len(via) > 0 {
				prev := via[len(via)-1].URL
				if prev.Scheme == "https" && req.URL.Scheme == "http" {
					return fmt.Errorf(
						"https → http downgrade redirect refused (%s → %s — use --allow-scheme-downgrade to permit)",
						prev.Host, req.URL,
					)
				}
			}
			if err := validateRequestURL(req.URL, p); err != nil {
				return fmt.Errorf("redirect refused: %w (run `perch help --allow-private-ips` for details)", err)
			}
			return nil
		},
	}
}

// validateRequestURL is the SSRF + host-allowlist gate. Called once on
// the initial request and again in CheckRedirect for every hop. Two
// independent checks compose AND-wise:
//
//   1. Host allowlist (when set) — every URL host (or host:port) must
//      match an entry. Redirects to a host not on the list are refused.
//   2. SSRF — resolve host to A/AAAA records, reject if ANY land in a
//      private / loopback / link-local / unspecified range. Defense
//      against DNS rebinding (multi-A responses) included.
func validateRequestURL(u *url.URL, p interpreter.HTTPPolicy) error {
	host := u.Hostname()
	if host == "" {
		return fmt.Errorf("empty host in URL %q", u)
	}
	// 1. Host allowlist (most restrictive — checked first).
	if len(p.AllowedHosts) > 0 && !hostInAllowlist(u, p.AllowedHosts) {
		return fmt.Errorf(
			"host %q is not in --allow-host allowlist (allowed: %s)",
			u.Host, strings.Join(p.AllowedHosts, ", "),
		)
	}
	// 2. SSRF — skipped only when --allow-private-ips is explicit.
	if p.AllowPrivateIPs {
		return nil
	}
	// Literal IP — check directly without a DNS round trip.
	if ip := net.ParseIP(host); ip != nil {
		if msg := privateIPCategory(ip); msg != "" {
			return fmt.Errorf("%s is a %s address (use --allow-private-ips to permit)", ip, msg)
		}
		return nil
	}
	// Hostname — resolve and check every record. A multi-A response
	// where any record is private is treated as private (DNS rebinding
	// defense).
	ips, err := net.LookupIP(host)
	if err != nil {
		// DNS failure — don't second-guess; let the actual request
		// surface the error in context.
		return nil
	}
	for _, ip := range ips {
		if msg := privateIPCategory(ip); msg != "" {
			return fmt.Errorf("%s resolves to %s (%s — use --allow-private-ips to permit)", host, ip, msg)
		}
	}
	return nil
}

// hostInAllowlist reports whether the URL's host (or host:port if the
// allowlist has port-specific entries) matches any allowlist entry.
//
// Match rules (each entry tried independently — first match wins):
//
//   - exact:           `api.github.com`     ↔ api.github.com
//   - single-label *:  `*.example.com`      ↔ api.example.com (NOT a.b.example.com)
//   - host:port:       `localhost:8080`     ↔ localhost on port 8080 only
//   - IP literal:      `10.0.0.1`           ↔ that exact IP
//
// Comparison is case-insensitive on the host. Port comparison is exact.
func hostInAllowlist(u *url.URL, allow []string) bool {
	hostLower := strings.ToLower(u.Hostname())
	port := u.Port()
	hostPort := strings.ToLower(u.Host) // includes port if present
	for _, pat := range allow {
		pat = strings.ToLower(strings.TrimSpace(pat))
		if pat == "" {
			continue
		}
		// host:port entry — require exact match on both
		if strings.Contains(pat, ":") && !strings.HasPrefix(pat, "[") {
			// Strip [ipv6] brackets from caller for comparison
			if pat == hostPort {
				return true
			}
			continue
		}
		// host-only entry: match against host, ignore the URL's port
		if hostMatchesPattern(pat, hostLower) {
			return true
		}
		// If allowlist entry has no port, port part is unrestricted —
		// already handled by hostMatchesPattern.
		_ = port
	}
	return false
}

// hostMatchesPattern handles the exact + single-label-wildcard cases.
// `*.example.com` matches `api.example.com` but NOT `a.b.example.com`
// (cookie-style single-label scoping — the safer default).
func hostMatchesPattern(pattern, host string) bool {
	if pattern == host {
		return true
	}
	if !strings.HasPrefix(pattern, "*.") {
		return false
	}
	suffix := pattern[1:] // includes the leading "."
	if !strings.HasSuffix(host, suffix) {
		return false
	}
	prefix := host[:len(host)-len(suffix)]
	if prefix == "" || strings.Contains(prefix, ".") {
		return false // empty prefix or multi-label prefix
	}
	return true
}

// privateIPCategory returns a non-empty descriptive string when the IP
// falls in a range we won't talk to by default; "" when it's public.
// Categories ordered to give the most useful error first.
func privateIPCategory(ip net.IP) string {
	switch {
	case ip.IsUnspecified():
		return "unspecified" // 0.0.0.0 / ::
	case ip.IsLoopback():
		return "loopback" // 127.0.0.0/8, ::1
	case ip.IsLinkLocalUnicast() || ip.IsLinkLocalMulticast():
		// Includes 169.254.0.0/16 — the AWS / GCP / Azure metadata service.
		return "link-local"
	case ip.IsPrivate():
		// RFC 1918: 10/8, 172.16/12, 192.168/16. IPv6: fc00::/7.
		return "private (RFC 1918 / ULA)"
	case ip.IsInterfaceLocalMulticast() || ip.IsMulticast():
		return "multicast"
	}
	return ""
}

// runHTTP encapsulates the validate-then-dispatch flow shared by every
// HTTP op. Returns the response (caller closes Body) or an error.
func runHTTP(i *interpreter.Interpreter, req *http.Request) (*http.Response, error) {
	p := httpPolicy(i)
	if err := validateRequestURL(req.URL, p); err != nil {
		return nil, fmt.Errorf("blocked: %w (run `perch help --allow-private-ips` for details)", err)
	}
	return newHTTPClient(p).Do(req)
}

// ─── ops ─────────────────────────────────────────────────────────────

func opHTTPGet(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	urlStr := argString(args, "url", "_0")
	req, err := http.NewRequest("GET", urlStr, nil)
	if err != nil {
		return "", err
	}
	resp, err := runHTTP(i, req)
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()
	body, err := io.ReadAll(resp.Body)
	if err != nil {
		return "", err
	}
	return string(body), nil
}

func opHTTPMethod(method string) interpreter.Handler {
	return func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		urlStr := argString(args, "url", "_0")
		body := argString(args, "body", "_1")
		req, err := http.NewRequest(method, urlStr, strings.NewReader(body))
		if err != nil {
			return "", err
		}
		req.Header.Set("Content-Type", "application/json")
		resp, err := runHTTP(i, req)
		if err != nil {
			return "", err
		}
		defer resp.Body.Close()
		out, err := io.ReadAll(resp.Body)
		if err != nil {
			return "", err
		}
		return string(out), nil
	}
}

func opDownload(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	urlStr := argString(args, "url")
	dst := resolve(argString(args, "dst"), b)
	req, err := http.NewRequest("GET", urlStr, nil)
	if err != nil {
		return nil, err
	}
	resp, err := runHTTP(i, req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	out, err := os.Create(dst)
	if err != nil {
		return nil, err
	}
	defer out.Close()
	_, err = io.Copy(out, resp.Body)
	return nil, err
}
