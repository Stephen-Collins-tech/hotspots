def single_except_clause(x):
    """Try/except with a single except clause."""
    try:
        result = 10 / x
    except ZeroDivisionError:
        return 0
    return result


def multiple_except_clauses(x):
    """Try/except with multiple except clauses."""
    try:
        result = 10 / x
        data = int(x)
    except ZeroDivisionError:
        return 0
    except TypeError:
        return -1
    except ValueError:
        return -2
    return result


def except_with_finally(filename):
    """Try/except/finally block."""
    file = None
    try:
        file = open(filename)
        data = file.read()
    except FileNotFoundError:
        return None
    except IOError:
        return ""
    finally:
        if file:
            file.close()
    return data


def nested_try_blocks(x, y):
    """Nested try/except blocks."""
    try:
        try:
            result = x / y
        except ZeroDivisionError:
            result = 0
    except TypeError:
        result = -1
    return result
