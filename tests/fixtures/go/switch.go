package fixtures

// Simple switch
// Expected: CC=1 (base) + 3 (cases) = 4, ND=1, FO=0, NS=0
func SimpleSwitch(x int) string {
	switch x {
	case 1:
		return "one"
	case 2:
		return "two"
	default:
		return "other"
	}
}

// Switch without default
// Expected: CC=1 (base) + 2 (cases) = 3, ND=1, FO=0, NS=0
func SwitchNoDefault(x int) {
	switch x {
	case 1:
		_ = "one"
	case 2:
		_ = "two"
	}
}

// Switch with fallthrough
// Expected: CC=1 (base) + 3 (cases) = 4, ND=1, FO=0, NS=0
func SwitchWithFallthrough(x int) int {
	result := 0
	switch x {
	case 1:
		result = 1
		fallthrough
	case 2:
		result = 2
	default:
		result = -1
	}
	return result
}

// Nested switch
// Expected: CC=1 (base) + 2 (outer cases) + 2 (inner cases) = 5, ND=2, FO=0, NS=0
func NestedSwitch(x, y int) {
	switch x {
	case 1:
		switch y {
		case 1:
			_ = "1,1"
		case 2:
			_ = "1,2"
		}
	case 2:
		_ = "2"
	}
}

// Expression switch
// Expected: CC=1 (base) + 2 (cases) = 3, ND=1, FO=0, NS=0
func ExpressionSwitch(x int) {
	switch {
	case x > 0:
		_ = "positive"
	case x < 0:
		_ = "negative"
	}
}

// Type switch
// Expected: CC=1 (base) + 3 (cases) = 4, ND=1, FO=0, NS=0
func TypeSwitch(x interface{}) {
	switch x.(type) {
	case int:
		_ = "int"
	case string:
		_ = "string"
	default:
		_ = "other"
	}
}

// Switch with multiple values per case
// Expected: CC=1 (base) + 2 (cases) = 3, ND=1, FO=0, NS=0
func SwitchMultipleValues(x int) {
	switch x {
	case 1, 2, 3:
		_ = "low"
	case 4, 5, 6:
		_ = "high"
	}
}
