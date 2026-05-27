// format.wasm — formats a numeric `total` field as a currency string.
package main

import (
	"encoding/json"
	"fmt"
	"os"
	"strings"
)

var currencySymbol = map[string]string{
	"USD": "$",
	"EUR": "€",
	"GBP": "£",
	"JPY": "¥",
}

func main() {
	raw, err := os.ReadFile("/ro/data/input.json")
	if err != nil {
		fmt.Fprintln(os.Stderr, "format: read input:", err)
		os.Exit(1)
	}
	var in struct {
		Total    float64 `json:"total"`
		Currency string  `json:"currency"`
	}
	if err := json.Unmarshal(raw, &in); err != nil {
		fmt.Fprintln(os.Stderr, "format: parse:", err)
		os.Exit(1)
	}
	if in.Currency == "" {
		in.Currency = "USD"
	}
	sym := currencySymbol[strings.ToUpper(in.Currency)]
	if sym == "" {
		sym = in.Currency + " "
	}
	out := map[string]any{
		"plugin":    "format",
		"total":     in.Total,
		"currency":  in.Currency,
		"formatted": fmt.Sprintf("%s%.2f", sym, in.Total),
	}
	enc, _ := json.MarshalIndent(out, "", "  ")
	fmt.Println(string(enc))
}
