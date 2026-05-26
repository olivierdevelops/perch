package ops

import (
	"fmt"
	"net"
	"os"
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
		return ips, nil
	}
	m["port_check"] = func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		// let ok = port_check "127.0.0.1" 8080
		addr := fmt.Sprintf("%s:%s", argString(args, "host", "_0"), argString(args, "port", "_1"))
		conn, err := net.DialTimeout("tcp", addr, 2*time.Second)
		if err != nil {
			return false, nil
		}
		conn.Close()
		return true, nil
	}
}
