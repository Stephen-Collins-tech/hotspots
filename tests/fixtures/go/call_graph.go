package callgraph

// helper has fo=0 (no calls)
func helper() int {
	return 1
}

// middle has fo=1 (calls helper twice, deduplicated to 1 unique callee)
func middle() int {
	return helper() + helper()
}

// top has fo=2 (calls helper + middle)
func top() int {
	return helper() + middle()
}
