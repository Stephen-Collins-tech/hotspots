int add(int a, int b) {
    return a + b;
}

int multiply(int a, int b) {
    int result = a * b;
    return result;
}

void noop(void) {
}

int absolute_value(int x) {
    if (x < 0) {
        return -x;
    }
    return x;
}
