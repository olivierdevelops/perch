package ops

import (
	"io"
	"net/http"
	"os"
	"strings"

	"github.com/luowensheng/perch/infra/interpreter"
)

func registerHTTP(m map[string]interpreter.Handler) {
	m["http_get"] = opHTTPGet
	m["http_post"] = opHTTPMethod("POST")
	m["http_put"] = opHTTPMethod("PUT")
	m["http_delete"] = opHTTPMethod("DELETE")
	m["download"] = opDownload
}

func opHTTPMethod(method string) interpreter.Handler {
	return func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		url := argString(args, "url", "_0")
		body := argString(args, "body", "_1")
		req, err := http.NewRequest(method, url, strings.NewReader(body))
		if err != nil {
			return "", err
		}
		req.Header.Set("Content-Type", "application/json")
		resp, err := http.DefaultClient.Do(req)
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

func opHTTPGet(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	url := argString(args, "url", "_0")
	resp, err := http.Get(url)
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

func opDownload(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	url := argString(args, "url")
	dst := resolve(argString(args, "dst"), b)
	resp, err := http.Get(url)
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
