// discount.wasm — applies a tier-based discount.
//
// Plugin contract:
//   reads:  /ro/data/input.json  (expects {"price":N,"qty":N})
//   writes: JSON to stdout
package main

import (
	"encoding/json"
	"fmt"
	"os"
)

func main() {
	raw, err := os.ReadFile("/ro/data/input.json")
	if err != nil {
		fmt.Fprintln(os.Stderr, "discount: read input:", err)
		os.Exit(1)
	}
	var in struct {
		Price float64 `json:"price"`
		Qty   int     `json:"qty"`
	}
	if err := json.Unmarshal(raw, &in); err != nil {
		fmt.Fprintln(os.Stderr, "discount: parse:", err)
		os.Exit(1)
	}
	rate := 0.0
	tier := "none"
	switch {
	case in.Qty >= 100:
		rate, tier = 0.20, "bulk-100+"
	case in.Qty >= 10:
		rate, tier = 0.10, "wholesale-10+"
	case in.Qty >= 3:
		rate, tier = 0.05, "multi-3+"
	}
	subtotal := in.Price * float64(in.Qty)
	out := map[string]any{
		"plugin":   "discount",
		"qty":      in.Qty,
		"subtotal": subtotal,
		"tier":     tier,
		"discount": subtotal * rate,
		"after":    subtotal * (1 - rate),
	}
	enc, _ := json.MarshalIndent(out, "", "  ")
	fmt.Println(string(enc))
}
