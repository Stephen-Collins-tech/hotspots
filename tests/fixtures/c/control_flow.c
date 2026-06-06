int classify(int x) {
    if (x > 0) {
        return 1;
    } else if (x < 0) {
        return -1;
    } else {
        return 0;
    }
}

int braceless_if(int x) {
    if (x > 0)
        return 1;
    else
        return 0;
}

int switch_days(int day) {
    switch (day) {
        case 0: return 0;
        case 6: return 0;
        default: return 1;
    }
}

int ternary_example(int x) {
    return x > 0 ? x : -x;
}

int boolean_ops(int a, int b, int c) {
    if (a > 0 && b > 0) {
        return 1;
    }
    if (a < 0 || b < 0) {
        return -1;
    }
    return 0;
}

int nested_ifs(int x, int y) {
    if (x > 0) {
        if (y > 0) {
            return 1;
        } else {
            return 2;
        }
    } else {
        if (y > 0) {
            return 3;
        } else {
            return 4;
        }
    }
}
