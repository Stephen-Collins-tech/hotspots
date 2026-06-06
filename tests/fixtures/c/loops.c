int sum_while(int n) {
    int sum = 0;
    while (n > 0) {
        sum += n;
        n--;
    }
    return sum;
}

int sum_for(int n) {
    int sum = 0;
    int i;
    for (i = 1; i <= n; i++) {
        sum += i;
    }
    return sum;
}

int sum_do_while(int n) {
    int sum = 0;
    do {
        sum += n;
        n--;
    } while (n > 0);
    return sum;
}

int loop_with_break(int n) {
    int i;
    for (i = 0; i < n; i++) {
        if (i == 5) break;
    }
    return i;
}

int loop_with_continue(int n) {
    int sum = 0;
    int i;
    for (i = 0; i < n; i++) {
        if (i % 2 == 0) continue;
        sum += i;
    }
    return sum;
}
