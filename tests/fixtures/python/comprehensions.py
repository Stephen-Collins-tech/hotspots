def simple_list_comp(items):
    """Simple list comprehension without filter."""
    return [x * 2 for x in items]


def filtered_list_comp(items):
    """List comprehension with filter (adds to CC)."""
    return [x for x in items if x > 0]


def dict_comprehension_filtered(items):
    """Dictionary comprehension with filter."""
    return {x: x**2 for x in items if x % 2 == 0}


def set_comprehension_filtered(items):
    """Set comprehension with filter."""
    return {x for x in items if x > 10}


def nested_comp(matrix):
    """Nested list comprehension without filters."""
    return [[cell * 2 for cell in row] for row in matrix]


def nested_comp_with_filter(matrix):
    """Nested comprehension with filter in inner comp."""
    return [[cell for cell in row if cell > 0] for row in matrix]


def complex_comprehension(data):
    """Comprehension with multiple filters."""
    return [
        x * y
        for x in data
        if x > 0
        for y in range(10)
        if y % 2 == 0
    ]


def generator_expression(items):
    """Generator expression with filter."""
    return (x**2 for x in items if x > 0)
