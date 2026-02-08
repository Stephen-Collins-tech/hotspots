def while_loop(n):
    """A simple while loop."""
    i = 0
    while i < n:
        i += 1
    return i


def for_loop_with_break(items):
    """A for loop with a break statement."""
    result = None
    for item in items:
        if item > 10:
            result = item
            break
    return result


def for_loop_with_continue(items):
    """A for loop with a continue statement."""
    count = 0
    for item in items:
        if item < 0:
            continue
        count += 1
    return count


def nested_loops(matrix):
    """Nested loops with control flow."""
    total = 0
    for row in matrix:
        for col in row:
            if col == 0:
                continue
            if col > 100:
                break
            total += col
    return total


async def async_for_loop(stream):
    """An async for loop."""
    results = []
    async for item in stream:
        if item is None:
            break
        results.append(item)
    return results
