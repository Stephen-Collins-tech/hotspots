public class Loops {
    public int whileLoop(int n) {
        int i = 0;
        while (i < n) {
            i++;
        }
        return i;
    }

    public int doWhileLoop(int n) {
        int i = 0;
        do {
            i++;
        } while (i < n);
        return i;
    }

    public int forLoopWithBreak(int[] items) {
        for (int item : items) {
            if (item > 10) {
                break;
            }
        }
        return items[0];
    }

    public void nestedLoops(int[][] matrix) {
        for (int[] row : matrix) {
            for (int col : row) {
                if (col == 0) {
                    continue;
                }
            }
        }
    }
}
