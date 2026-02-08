public class SwitchAndTernary {
    public String traditionalSwitch(int value) {
        switch (value) {
            case 0:
                return "zero";
            case 1:
                return "one";
            case 2:
                return "two";
            default:
                return "other";
        }
    }

    public String ternaryExpression(int x) {
        return x > 0 ? "positive" : "non-positive";
    }

    public boolean booleanOperators(boolean a, boolean b, boolean c) {
        if (a && b || c) {
            return true;
        }
        return false;
    }
}
