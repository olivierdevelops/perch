// perch is a cross-platform command runner. The real entry point is
// orchestrator.Run; this file exists so the binary builds from the
// repo root (`go install github.com/olivierdevelops/perch@latest`).
package main

import "github.com/olivierdevelops/perch/orchestrator"

func main() {
	orchestrator.Run()
}
