public class Simple {
    public int simpleMethod(int x) {
        return x + 1;
    }

    public int withEarlyReturn(int x) {
        if (x < 0) {
            return 0;
        }
        return x;
    }
}
