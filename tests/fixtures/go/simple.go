package fixtures

// Simple function with minimal complexity
// Expected: CC=1, ND=0, FO=0, NS=0
func Simple() {
	x := 1
	_ = x
}

// Function with single branch
// Expected: CC=2, ND=1, FO=0, NS=0
func SingleBranch(x int) {
	if x > 0 {
		x++
	}
}

// Function with if/else
// Expected: CC=2, ND=1, FO=0, NS=0
func IfElse(x int) int {
	if x > 0 {
		return x + 1
	} else {
		return x - 1
	}
}

// Function with early return
// Expected: CC=2, ND=1, FO=0, NS=1 (one early return)
func EarlyReturn(x int) int {
	if x < 0 {
		return -1
	}
	return x * 2
}

// Function with multiple returns
// Expected: CC=3, ND=1, FO=0, NS=2 (two early returns)
func MultipleReturns(x int) int {
	if x < 0 {
		return -1
	}
	if x == 0 {
		return 0
	}
	return x * 2
}
