int cleanup_pattern(int x) {
    int result = 0;
    if (x < 0) goto done;
    result = x * 2;
    done:
    return result;
}

int multi_goto(int x, int y) {
    if (x < 0) goto error;
    if (y < 0) goto error;
    return x + y;
    error:
    return -1;
}

void loop_goto(int n) {
    int i = 0;
    top:
    if (i >= n) goto end;
    i++;
    goto top;
    end:
    return;
}
