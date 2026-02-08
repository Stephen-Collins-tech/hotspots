public class Classes {
    private int value;

    public Classes(int value) {
        this.value = value;
    }

    public int instanceMethod(int x) {
        return x + this.value;
    }

    public static int staticMethod(int x) {
        return x * 2;
    }

    class InnerClass {
        public int innerMethod(int y) {
            return y + value;
        }
    }
}
