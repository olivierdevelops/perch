// Error-handling ops: try / catch / finally + match / case / else.
//
// The body of a `try` block is one flat list of ops with sentinel
// dividers — `_catch` and `_finally`. The handler walks the list,
// splits on the sentinels, and dispatches:
//
//   1. Try-body: every op before _catch (and before _finally if no _catch).
//   2. Catch-body: every op after _catch and before _finally. Only runs
//      if the try-body errored; populates ${BIND.kind} etc.
//   3. Finally-body: every op after _finally. Runs unconditionally,
//      AFTER catch (or after try-body if no catch and no error).
//
// match works the same way: `_case <value>` and `_else` are sentinels;
// the handler picks the first matching case (or _else) and runs that
// arm's body.
package ops

import (
	"github.com/luowensheng/perch/domain"
	"github.com/luowensheng/perch/infra/interpreter"
)

func registerErrorOps(m map[string]interpreter.Handler) {
	m["try"] = opTry
	m["match"] = opMatch

	// Sentinel handlers — these ops are markers inside try/match bodies,
	// not standalone ops. Calling them outside their parent block is an
	// error.
	m["_catch"] = sentinelErr("catch", "try")
	m["_finally"] = sentinelErr("finally", "try")
	m["_case"] = sentinelErr("case", "match")
	m["_else"] = sentinelErr("else", "match")
}

func sentinelErr(name, parent string) interpreter.Handler {
	return func(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
		return nil, domain.NewOpError(name, domain.ErrUnclassified,
			name+" is only valid inside a "+parent+" block")
	}
}

// opTry executes the try-body; on error, populates ${BIND.*} bindings
// and runs the catch-body. The finally-body runs unconditionally last.
// If the catch-body itself errors (or there is no catch and the
// try-body errored), the error propagates after finally runs.
func opTry(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	body, _ := args["_body"].([]domain.Op)
	tryBody, catchBody, finallyBody, catchBind := splitTryBody(body)

	// Run try-body; capture the error (if any) without aborting yet.
	tryErr := i.RunOps(tryBody, b)

	// Catch arm — only if try errored AND a catch arm exists.
	var catchErr error
	if tryErr != nil && catchBody != nil {
		oe := domain.ClassifyError("", tryErr)
		populateErrBindings(b, catchBind, oe)
		catchErr = i.RunOps(catchBody, b)
		// Whatever bindings catch produced/clobbered remain visible
		// to the finally body. catch-body errors take precedence over
		// the original try error (the user's recovery code "decided"
		// to re-raise; their error wins).
		if catchErr != nil {
			tryErr = catchErr
		} else {
			tryErr = nil // handled
		}
	}

	// Finally — always runs.
	var finalErr error
	if finallyBody != nil {
		finalErr = i.RunOps(finallyBody, b)
	}

	// Finally errors override everything else (otherwise users couldn't
	// observe a failure in their cleanup code).
	if finalErr != nil {
		return nil, finalErr
	}
	return nil, tryErr
}

// splitTryBody walks a try block's flat body and partitions it on
// `_catch` and `_finally` sentinel ops. Returns the three sections
// + the catch-binding name.
func splitTryBody(body []domain.Op) (tryBody, catchBody, finallyBody []domain.Op, catchBind string) {
	state := "try"
	catchBind = "err" // default if user wrote `catch err`
	for _, op := range body {
		switch op.Kind {
		case "_catch":
			state = "catch"
			if bind, ok := op.Args["bind"].(string); ok && bind != "" {
				catchBind = bind
			}
			if catchBody == nil {
				catchBody = []domain.Op{}
			}
			continue
		case "_finally":
			state = "finally"
			if finallyBody == nil {
				finallyBody = []domain.Op{}
			}
			continue
		}
		switch state {
		case "try":
			tryBody = append(tryBody, op)
		case "catch":
			catchBody = append(catchBody, op)
		case "finally":
			finallyBody = append(finallyBody, op)
		}
	}
	return
}

// populateErrBindings sets ${BIND.kind}, ${BIND.message}, ${BIND.code},
// ${BIND.op}, ${BIND.detail} so the catch body can discriminate.
func populateErrBindings(b *interpreter.Bindings, bind string, oe *domain.OpError) {
	if oe == nil {
		return
	}
	b.Set(bind+".kind", string(oe.Kind))
	b.Set(bind+".message", oe.Message)
	b.Set(bind+".code", oe.Code)
	b.Set(bind+".op", oe.Op)
	b.Set(bind+".detail", oe.Detail)
	// Plain ${err} → message, for the common "just rethrow" case.
	b.Set(bind, oe.Message)
}

// opMatch evaluates args.target and dispatches to the first matching
// `case` arm (or `else` arm if none match).
func opMatch(i *interpreter.Interpreter, b *interpreter.Bindings, args map[string]any) (any, error) {
	// Both `match "${X}"` (string form) and `match X` (ident form) land
	// in args.target — the ident form's `_target_var` is auto-resolved by
	// InterpolateArgs into `target` before we run.
	target := interpreter.ToStringValue(args["target"])
	body, _ := args["_body"].([]domain.Op)
	arms := splitMatchBody(body)

	for _, arm := range arms {
		if arm.isElse {
			continue
		}
		if arm.value == target {
			return nil, i.RunOps(arm.body, b)
		}
	}
	// No case matched — find the else arm.
	for _, arm := range arms {
		if arm.isElse {
			return nil, i.RunOps(arm.body, b)
		}
	}
	// No match, no else — silent no-op. (Strict-exhaustive mode is
	// future work; for now we mirror chained-if behavior.)
	return nil, nil
}

type matchArm struct {
	isElse bool
	value  string
	body   []domain.Op
}

// splitMatchBody walks a match block's flat body and partitions it on
// `_case` / `_else` sentinel ops. Each arm carries its match value (or
// isElse=true) and the body to run.
func splitMatchBody(body []domain.Op) []matchArm {
	var arms []matchArm
	var cur *matchArm
	for _, op := range body {
		switch op.Kind {
		case "_case":
			if cur != nil {
				arms = append(arms, *cur)
			}
			v := interpreter.ToStringValue(op.Args["value"])
			cur = &matchArm{value: v}
			continue
		case "_else":
			if cur != nil {
				arms = append(arms, *cur)
			}
			cur = &matchArm{isElse: true}
			continue
		}
		if cur == nil {
			// Op appears before any case/else — silently ignored.
			// (perch --check warns on this shape.)
			continue
		}
		cur.body = append(cur.body, op)
	}
	if cur != nil {
		arms = append(arms, *cur)
	}
	return arms
}
