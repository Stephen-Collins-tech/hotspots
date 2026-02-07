package fixtures

import (
	"fmt"
	"time"
)

// Function with defer
// Expected: CC=1, ND=0, FO=1 (cleanup), NS=1 (defer)
func WithDefer() {
	defer cleanup()
}

// Function with multiple defers
// Expected: CC=1, ND=0, FO=1 (unique cleanup), NS=3 (three defers)
func MultipleDefers() {
	defer cleanup()
	defer cleanup()
	defer cleanup()
}

// Function with conditional defer
// Expected: CC=2, ND=1, FO=1, NS=1
func ConditionalDefer(x int) {
	if x > 0 {
		defer cleanup()
	}
}

// Function with goroutine
// Expected: CC=1, ND=0, FO=1 (go statement), NS=0
func WithGoroutine() {
	go doWork()
}

// Function with multiple goroutines
// Expected: CC=1, ND=0, FO=2 (two unique go statements), NS=0
func MultipleGoroutines() {
	go doWork()
	go doOtherWork()
}

// Function with goroutine and defer
// Expected: CC=1, ND=0, FO=2 (cleanup + go), NS=1 (defer)
func GoroutineAndDefer() {
	defer cleanup()
	go doWork()
}

// Function with panic
// Expected: CC=2, ND=1, FO=1 (panic approximated), NS=2 (early return + panic)
func WithPanic(x int) {
	if x < 0 {
		panic("negative value")
	}
}

// Function with recover
// Expected: CC=1, ND=0, FO=1 (recover), NS=1 (defer)
func WithRecover() {
	defer func() {
		if r := recover(); r != nil {
			_ = r
		}
	}()
}

// Select statement
// Expected: CC=1 (base) + 2 (cases) = 3, ND=1, FO=0, NS=0
func SimpleSelect() {
	ch1 := make(chan int)
	ch2 := make(chan int)

	select {
	case <-ch1:
		_ = "ch1"
	case <-ch2:
		_ = "ch2"
	}
}

// Select with default
// Expected: CC=1 (base) + 3 (cases) = 4, ND=1, FO=1 (make), NS=0
func SelectWithDefault() {
	ch := make(chan int)

	select {
	case v := <-ch:
		_ = v
	case ch <- 1:
		_ = "sent"
	default:
		_ = "default"
	}
}

// Select in loop
// Expected: CC=1 (base) + 1 (loop) + 2 (select cases) = 4, ND=2, FO=2 (make + time.After), NS=0
func SelectInLoop() {
	ch := make(chan int)
	for {
		select {
		case v := <-ch:
			_ = v
		case <-time.After(time.Second):
			return
		}
	}
}

// Complex function with all Go features
// Expected: High CC, ND, FO, NS due to multiple features
func ComplexGoFunction(x int) error {
	// Defer
	defer cleanup()

	// Early return
	if x < 0 {
		return fmt.Errorf("negative")
	}

	// Loop with goroutine
	for i := 0; i < x; i++ {
		go func(val int) {
			doWork()
		}(i)

		// Nested switch
		switch i {
		case 0:
			continue
		case 1:
			if i > 0 {
				break
			}
		default:
			_ = i
		}
	}

	// Select
	ch := make(chan int)
	select {
	case v := <-ch:
		return fmt.Errorf("got %d", v)
	default:
		_ = "default"
	}

	return nil
}

// Helper functions
func cleanup()      {}
func doWork()       {}
func doOtherWork()  {}
