package fixtures

// Simple for loop
// Expected: CC=1 (base) + 1 (loop) = 2, ND=1, FO=0, NS=0
func SimpleLoop() {
	for i := 0; i < 10; i++ {
		_ = i
	}
}

// For loop with condition
// Expected: CC=2 (base + loop + if), ND=2 (loop->if), FO=0, NS=0
func LoopWithCondition() {
	for i := 0; i < 10; i++ {
		if i > 5 {
			_ = i
		}
	}
}

// Nested loops
// Expected: CC=2 (base + 2 loops), ND=2, FO=0, NS=0
func NestedLoops() {
	for i := 0; i < 10; i++ {
		for j := 0; j < 10; j++ {
			_ = i + j
		}
	}
}

// Loop with break
// Expected: CC=3 (base + loop + if), ND=2, FO=0, NS=0
func LoopWithBreak() {
	for i := 0; i < 10; i++ {
		if i > 5 {
			break
		}
	}
}

// Loop with continue
// Expected: CC=3 (base + loop + if), ND=2, FO=0, NS=0
func LoopWithContinue() {
	for i := 0; i < 10; i++ {
		if i%2 == 0 {
			continue
		}
		_ = i
	}
}

// Range loop
// Expected: CC=1 (base) + 1 (loop) = 2, ND=1, FO=0, NS=0
func RangeLoop() {
	items := []int{1, 2, 3, 4, 5}
	for _, item := range items {
		_ = item
	}
}

// While-style loop
// Expected: CC=1 (base) + 1 (loop) = 2, ND=1, FO=0, NS=0
func WhileStyleLoop() {
	i := 0
	for i < 10 {
		i++
	}
}

// Infinite loop with break
// Expected: CC=2 (base + loop + if), ND=2, FO=0, NS=0
func InfiniteLoop() {
	i := 0
	for {
		if i > 10 {
			break
		}
		i++
	}
}
