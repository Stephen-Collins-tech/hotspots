package fixtures

// Type for methods
type Calculator struct {
	value int
}

// Simple method
// Expected: CC=1, ND=0, FO=0, NS=0
func (c *Calculator) GetValue() int {
	return c.value
}

// Method with logic
// Expected: CC=2, ND=1, FO=0, NS=1 (early return)
func (c *Calculator) SetValue(v int) {
	if v < 0 {
		return
	}
	c.value = v
}

// Method with calculations
// Expected: CC=3, ND=1, FO=0, NS=2
func (c *Calculator) Add(x int) int {
	if x < 0 {
		return c.value
	}
	if c.value+x > 100 {
		return 100
	}
	return c.value + x
}

// Value receiver method
// Expected: CC=1, ND=0, FO=0, NS=0
func (c Calculator) IsPositive() bool {
	return c.value > 0
}

// Method with complex logic
// Expected: High CC, ND
func (c *Calculator) Process(x int) int {
	result := c.value

	for i := 0; i < x; i++ {
		if i%2 == 0 {
			switch i {
			case 0:
				result++
			case 2:
				result += 2
			default:
				result += i
			}
		} else {
			result--
		}
	}

	return result
}

// Interface type
type Worker interface {
	Work() error
	Stop()
}

// Concrete implementation
type SimpleWorker struct {
	running bool
}

// Method implementing interface
// Expected: CC=2, ND=1, FO=1 (doWork), NS=1 (early return)
func (w *SimpleWorker) Work() error {
	if !w.running {
		return nil
	}
	go doWork()
	return nil
}

// Another interface method
// Expected: CC=1, ND=0, FO=0, NS=0
func (w *SimpleWorker) Stop() {
	w.running = false
}

// Generic type (Go 1.18+)
type Container[T any] struct {
	items []T
}

// Generic method
// Expected: CC=1, ND=0, FO=0, NS=0
func (c *Container[T]) Add(item T) {
	c.items = append(c.items, item)
}

// Generic method with logic
// Expected: CC=2, ND=1, FO=0, NS=1
func (c *Container[T]) Get(index int) (T, bool) {
	var zero T
	if index < 0 || index >= len(c.items) {
		return zero, false
	}
	return c.items[index], true
}
