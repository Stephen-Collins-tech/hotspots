package fixtures

// Function with AND operator
// Expected: CC=1 (base) + 1 (if) + 1 (&&) = 3, ND=1, FO=0, NS=0
func WithAnd(x, y int) {
	if x > 0 && y > 0 {
		_ = "both positive"
	}
}

// Function with OR operator
// Expected: CC=1 (base) + 1 (if) + 1 (||) = 3, ND=1, FO=0, NS=0
func WithOr(x, y int) {
	if x > 0 || y > 0 {
		_ = "at least one positive"
	}
}

// Function with multiple boolean operators
// Expected: CC=1 (base) + 1 (if) + 3 (&&, &&, ||) = 5, ND=1, FO=0, NS=1
func MultipleBooleanOps(x, y, z int) int {
	if x > 0 && y > 0 && z > 0 || x < 0 {
		return 1
	}
	return 0
}

// Function with complex boolean expression
// Expected: CC=1 (base) + 1 (if) + 4 (&&, ||, &&, ||) = 6, ND=1, FO=0, NS=1
func ComplexBooleanExpression(a, b, c, d int) bool {
	if (a > 0 && b > 0) || (c > 0 && d > 0) {
		return true
	}
	return false
}

// Nested conditions with boolean operators
// Expected: High CC due to nested ifs and boolean operators, ND=3
func NestedWithBooleanOps(x, y, z int) int {
	if x > 0 {
		if y > 0 && z > 0 {
			if x > 10 || y > 10 {
				return 1
			}
		}
	}
	return 0
}

// Switch with boolean operators in cases
// Expected: CC includes switch cases and boolean operators
func SwitchWithBooleanOps(x, y int) {
	switch {
	case x > 0 && y > 0:
		_ = "both positive"
	case x > 0 || y > 0:
		_ = "one positive"
	default:
		_ = "none positive"
	}
}

// Loop with boolean operators
// Expected: CC includes loop and boolean operators
func LoopWithBooleanOps(items []int) int {
	count := 0
	for i, item := range items {
		if i > 0 && item > 0 || item < 0 {
			count++
		}
	}
	return count
}

// Deeply nested function
// Expected: ND=5 (five levels)
func DeeplyNested(x int) {
	if x > 0 {
		for i := 0; i < x; i++ {
			if i > 5 {
				switch i {
				case 6:
					if i%2 == 0 {
						_ = "deep"
					}
				}
			}
		}
	}
}

// Pathological complexity
// Expected: Very high CC, ND, FO, NS
func PathologicalComplexity(x, y, z int) int {
	result := 0

	// Multiple early returns
	if x < 0 {
		return -1
	}
	if y < 0 {
		return -2
	}
	if z < 0 {
		return -3
	}

	// Nested loops with conditions
	for i := 0; i < x; i++ {
		for j := 0; j < y; j++ {
			if i > 0 && j > 0 || i < 0 {
				switch i + j {
				case 1:
					result++
				case 2:
					result += 2
				case 3:
					if z > 0 && result > 0 {
						result *= 2
					}
				default:
					result--
				}
			}
		}
	}

	// More boolean operators
	if result > 100 && x > 10 || result < 0 && y > 5 {
		return result * 2
	}

	return result
}
