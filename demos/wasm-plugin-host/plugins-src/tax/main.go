// tax.wasm — adds 10% tax to `price` field, emits `total`.
//
// Standard perch-plugin contract:
//   reads:  /ro/data/input.json
//   writes: JSON to stdout
//   exits:  0 on success, 1 on error
//
// Build:
//   GOOS=wasip1 GOARCH=wasm go build -o ../../plugins/tax.wasm .
package main

import (
	"encoding/json"
	"fmt"
	"os"
)

const TaxRate = 0.10

func main() {
	raw, err := os.ReadFile("/ro/data/input.json")
	if err != nil {
		fmt.Fprintln(os.Stderr, "tax: read input:", err)
		os.Exit(1)
	}
	var in struct {
		Price float64 `json:"price"`
	}
	if err := json.Unmarshal(raw, &in); err != nil {
		fmt.Fprintln(os.Stderr, "tax: parse:", err)
		os.Exit(1)
	}
	tax := in.Price * TaxRate
	out := map[string]any{
		"plugin": "tax",
		"price":  in.Price,
		"tax":    tax,
		"total":  in.Price + tax,
	}
	enc, _ := json.MarshalIndent(out, "", "  ")
	fmt.Println(string(enc))
}
