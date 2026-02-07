def with_context_manager(filename):
    """Function using a with statement (context manager)."""
    with open(filename) as f:
        return f.read()


def nested_context_managers(file1, file2):
    """Nested with statements."""
    with open(file1) as f1:
        with open(file2) as f2:
            return f1.read() + f2.read()


def list_comprehension_filtered(items):
    """List comprehension with if filter (should add to CC)."""
    return [x for x in items if x > 5]


def list_comprehension_no_filter(items):
    """List comprehension without filter (should NOT add to CC)."""
    return [x * 2 for x in items]


async def async_function():
    """An async function with async context manager."""
    async with get_connection() as conn:
        return await conn.fetch_data()


async def async_for_with_filter(stream):
    """Async for with conditional."""
    results = []
    async for item in stream:
        if item.is_valid:
            results.append(item)
    return results


def match_statement(value):
    """Match statement (Python 3.10+) with multiple cases."""
    match value:
        case 0:
            return "zero"
        case 1:
            return "one"
        case 2 | 3:
            return "two or three"
        case _:
            return "other"


def match_with_guard(point):
    """Match statement with guard clauses."""
    match point:
        case (0, 0):
            return "origin"
        case (x, 0) if x > 0:
            return "positive x-axis"
        case (0, y) if y > 0:
            return "positive y-axis"
        case _:
            return "elsewhere"
