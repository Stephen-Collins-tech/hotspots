def simple_function(x):
    """A simple function with no control flow."""
    return x + 1


def with_early_return(x):
    """A function with an early return (if statement)."""
    if x < 0:
        return 0
    return x


def multiple_returns(x, y):
    """A function with multiple return statements."""
    if x < 0:
        return -1
    if y < 0:
        return -2
    return x + y
