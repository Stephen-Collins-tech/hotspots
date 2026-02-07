def boolean_and(a, b, c):
    """Function with 'and' operator (adds to CC)."""
    if a and b:
        return True
    return False


def boolean_or(a, b, c):
    """Function with 'or' operator (adds to CC)."""
    if a or b:
        return True
    return False


def boolean_and_or(a, b, c):
    """Function with both 'and' and 'or' operators."""
    if a and b or c:
        return True
    return False


def complex_boolean(a, b, c, d):
    """Complex boolean expression."""
    if (a and b) or (c and d):
        return 1
    elif a or b:
        return 2
    return 3


def ternary_expression(x):
    """Ternary/conditional expression (adds to CC)."""
    return "positive" if x > 0 else "non-positive"


def nested_ternary(x, y):
    """Nested ternary expressions."""
    return "high" if x > 10 else ("medium" if x > 5 else "low")


def ternary_in_assignment(value):
    """Ternary expression in assignment."""
    result = value * 2 if value > 0 else 0
    return result


def boolean_in_return(a, b):
    """Boolean operators in return statement."""
    return a > 0 and b > 0 or a == b


def walrus_operator(items):
    """Walrus operator (assignment expression)."""
    if (n := len(items)) > 10:
        return n
    return 0
