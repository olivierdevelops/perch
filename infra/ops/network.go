package ops

import (
	"context"
	"fmt"
	"net"
	"net/http"
	"os"
	"strconv"
	"strings"
	"time"

	"github.com/luowensheng/perch/infra/interpreter"
)

func registerNetwork(m map[string]interpreter.Handler) {
	m["hostname"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		return os.Hostname()
	}
	m["dns_lookup"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		ips, err := net.LookupHost(argString(args, "host", "_0"))
		if err != nil {
			return nil, err
		}
		return strings.Join(ips, "\n"), nil
	}
	m["port_check"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		// `let ok = port_check "127.0.0.1" 8080` — true if something is
		// listening (i.e. a connect succeeds within 2s).
		addr := fmt.Sprintf("%s:%s", argString(args, "host", "_0"), argString(args, "port", "_1"))
		conn, err := net.DialTimeout("tcp", addr, 2*time.Second)
		if err != nil {
			return false, nil
		}
		conn.Close()
		return true, nil
	}
	m["port_free"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		port := argString(args, "port", "_0")
		ln, err := net.Listen("tcp", ":"+port)
		if err != nil {
			return false, nil
		}
		ln.Close()
		return true, nil
	}
	m["find_free_port"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		ln, err := net.Listen("tcp", ":0")
		if err != nil {
			return 0, err
		}
		defer ln.Close()
		return ln.Addr().(*net.TCPAddr).Port, nil
	}
	// wait_for_port "127.0.0.1" 6379 30   → true when reachable, false on timeout.
	m["wait_for_port"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		host := argString(args, "host", "_0")
		port := argString(args, "port", "_1")
		secs, _ := strconv.Atoi(argString(args, "timeout", "_2"))
		if secs <= 0 {
			secs = 30
		}
		deadline := time.Now().Add(time.Duration(secs) * time.Second)
		addr := host + ":" + port
		for time.Now().Before(deadline) {
			conn, err := net.DialTimeout("tcp", addr, 500*time.Millisecond)
			if err == nil {
				conn.Close()
				return true, nil
			}
			time.Sleep(250 * time.Millisecond)
		}
		return false, nil
	}
	// wait_for_url "http://localhost:8080/health" 30 → true when returns 2xx.
	m["wait_for_url"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		url := argString(args, "url", "_0")
		secs, _ := strconv.Atoi(argString(args, "timeout", "_1"))
		if secs <= 0 {
			secs = 30
		}
		client := &http.Client{Timeout: 2 * time.Second}
		deadline := time.Now().Add(time.Duration(secs) * time.Second)
		for time.Now().Before(deadline) {
			resp, err := client.Get(url)
			if err == nil {
				resp.Body.Close()
				if resp.StatusCode >= 200 && resp.StatusCode < 300 {
					return true, nil
				}
			}
			time.Sleep(500 * time.Millisecond)
		}
		return false, nil
	}
	m["http_status"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		urlStr := argString(args, "url", "_0")
		req, err := http.NewRequestWithContext(context.Background(), "HEAD", urlStr, nil)
		if err != nil {
			return 0, err
		}
		// Reuse the same policy-aware client as http_get / http_post:
		// SSRF check, redirect cap, scheme-downgrade guard.
		resp, err := runHTTP(i, req)
		if err != nil {
			return 0, nil
		}
		resp.Body.Close()
		return resp.StatusCode, nil
	}
	m["local_ip"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		// Dial a public address (no packets sent) to pick the outbound
		// interface's local IP. Falls back to scanning interfaces.
		conn, err := net.Dial("udp", "8.8.8.8:80")
		if err == nil {
			defer conn.Close()
			return conn.LocalAddr().(*net.UDPAddr).IP.String(), nil
		}
		ifaces, err := net.Interfaces()
		if err != nil {
			return "", err
		}
		for _, iface := range ifaces {
			if iface.Flags&net.FlagLoopback != 0 || iface.Flags&net.FlagUp == 0 {
				continue
			}
			addrs, _ := iface.Addrs()
			for _, a := range addrs {
				if ipnet, ok := a.(*net.IPNet); ok && ipnet.IP.To4() != nil {
					return ipnet.IP.String(), nil
				}
			}
		}
		return "", nil
	}
	m["public_ip"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		// Single-shot via ipify. Users wanting to avoid the network call
		// should provide their own via http_get + a different service.
		client := &http.Client{Timeout: 5 * time.Second}
		resp, err := client.Get("https://api.ipify.org")
		if err != nil {
			return "", err
		}
		defer resp.Body.Close()
		buf := make([]byte, 64)
		n, _ := resp.Body.Read(buf)
		return strings.TrimSpace(string(buf[:n])), nil
	}
	m["interfaces"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		ifs, err := net.Interfaces()
		if err != nil {
			return "", err
		}
		names := []string{}
		for _, n := range ifs {
			if n.Flags&net.FlagUp != 0 {
				names = append(names, n.Name)
			}
		}
		return strings.Join(names, "\n"), nil
	}
	m["mac_address"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		ifs, err := net.Interfaces()
		if err != nil {
			return "", err
		}
		for _, n := range ifs {
			if n.Flags&net.FlagUp != 0 && n.Flags&net.FlagLoopback == 0 && len(n.HardwareAddr) > 0 {
				return n.HardwareAddr.String(), nil
			}
		}
		return "", nil
	}
}
