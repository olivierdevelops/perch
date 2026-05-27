// shipping.wasm — calculates shipping based on weight + region.
package main

import (
	"encoding/json"
	"fmt"
	"os"
)

var regionMultiplier = map[string]float64{
	"domestic":      1.0,
	"international": 2.5,
	"express":       3.0,
}

func main() {
	raw, err := os.ReadFile("/ro/data/input.json")
	if err != nil {
		fmt.Fprintln(os.Stderr, "shipping: read input:", err)
		os.Exit(1)
	}
	var in struct {
		WeightKg float64 `json:"weight_kg"`
		Region   string  `json:"region"`
	}
	if err := json.Unmarshal(raw, &in); err != nil {
		fmt.Fprintln(os.Stderr, "shipping: parse:", err)
		os.Exit(1)
	}
	if in.Region == "" {
		in.Region = "domestic"
	}
	mul, ok := regionMultiplier[in.Region]
	if !ok {
		fmt.Fprintf(os.Stderr, "shipping: unknown region %q\n", in.Region)
		os.Exit(1)
	}
	base := 5.0 + in.WeightKg*2.5
	out := map[string]any{
		"plugin":  "shipping",
		"weight":  in.WeightKg,
		"region":  in.Region,
		"base":    base,
		"total":   base * mul,
	}
	enc, _ := json.MarshalIndent(out, "", "  ")
	fmt.Println(string(enc))
}
