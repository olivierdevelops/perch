// Risk scoring — convert a Report into a HIGH / MED / LOW / SAFE
// summary that non-experts can read at a glance. Used in the --scan
// output and (eventually) in the web UI's Scan tab.
//
// The score is intentionally coarse:
//
//   HIGH  — at least one HIGH-severity finding (sudo, ${proxy_args}
//           into shell, unvalidated interp into shell, etc.)
//           OR the program needs both shell + network without any
//           allowlist.
//   MED   — at least one MED-severity finding OR the program does
//           anything in two or more "powerful" categories without
//           explicit allowlists.
//   LOW   — needs only one category (shell, network, writes), or
//           has only LOW-severity findings.
//   SAFE  — pure ops; no shell, no network, no subprocess, no writes.
//
// This is intentionally NOT a security guarantee — it's a UI
// affordance for "is this worth carefully reading before I run?"
package scan

import "fmt"

// RiskScore is the coarse classification.
type RiskScore int

const (
	RiskSafe RiskScore = iota
	RiskLow
	RiskMed
	RiskHigh
)

func (s RiskScore) String() string {
	switch s {
	case RiskSafe:
		return "SAFE"
	case RiskLow:
		return "LOW"
	case RiskMed:
		return "MED"
	case RiskHigh:
		return "HIGH"
	}
	return "?"
}

// ScoreReport computes a coarse risk score plus the reasons that
// pushed it up from SAFE. The reasons are human-readable strings shown
// to users; they're not a stable API.
func ScoreReport(r Report) (RiskScore, []string) {
	score := RiskSafe
	var reasons []string

	// Capability-driven baseline.
	cats := 0
	if r.NeedsShell {
		cats++
		reasons = append(reasons, "executes shell")
	}
	if r.NeedsSubprocess {
		cats++
		reasons = append(reasons, "spawns subprocesses (pkg_install / kill / process_running)")
	}
	if r.NeedsNetwork {
		cats++
		hosts := ""
		if n := len(r.Hosts); n > 0 {
			hosts = fmt.Sprintf(" (%d host%s)", n, plural(n))
		}
		reasons = append(reasons, "network access"+hosts)
	}
	if r.NeedsWrite {
		cats++
		hosts := ""
		if n := len(r.WriteRoots); n > 0 {
			hosts = fmt.Sprintf(" (%d root%s)", n, plural(n))
		}
		reasons = append(reasons, "writes the filesystem"+hosts)
	}

	switch {
	case cats == 0:
		// Stays SAFE.
	case cats >= 2:
		score = RiskMed
	default:
		score = RiskLow
	}

	// Specific high-signal patterns push the score up.
	if r.HasShellSudo {
		score = RiskHigh
		reasons = append(reasons, "uses `sudo` (privilege escalation)")
	}
	if r.HasShellPipe {
		if score < RiskMed {
			score = RiskMed
		}
		reasons = append(reasons, "uses shell metacharacters (pipe / && / ; / $())")
	}
	if r.CatchForwards {
		score = RiskHigh
		reasons = append(reasons,
			"catch-all forwards ${proxy_args} to shell (any unknown verb → shell)")
	}

	// Findings escalate.
	for _, f := range r.Findings {
		switch f.Severity {
		case "high":
			if score < RiskHigh {
				score = RiskHigh
			}
		case "med":
			if score < RiskMed {
				score = RiskMed
			}
		}
	}

	if len(reasons) == 0 {
		reasons = []string{"no privileged operations — pure ops only"}
	}
	return score, reasons
}

// riskBadge renders the score as a short colored-by-letter label.
// Terminal coloring is intentionally NOT applied here (the caller's
// shell may or may not be a TTY); the badge is plain text.
func riskBadge(s RiskScore) string {
	switch s {
	case RiskSafe:
		return "🟢 SAFE  (pure ops — no shell, no network, no writes)"
	case RiskLow:
		return "🟡 LOW   (limited surface — review the capabilities below)"
	case RiskMed:
		return "🟠 MED   (multiple capabilities or shell metachars — review carefully)"
	case RiskHigh:
		return "🔴 HIGH  (sudo / proxy_args / privileged ops — read every command before running)"
	}
	return "?"
}

func plural(n int) string {
	if n == 1 {
		return ""
	}
	return "s"
}
