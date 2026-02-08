import java.io.File;
import java.io.IOException;
import java.util.Scanner;

public class Exceptions {
    public int multipleCatchClauses(int x) {
        try {
            int result = 10 / x;
            return result;
        } catch (ArithmeticException e) {
            return 0;
        } catch (Exception e) {
            return -1;
        } finally {
            // cleanup
        }
    }

    public String tryWithResources(String filename) {
        try (Scanner sc = new Scanner(new File(filename))) {
            return sc.nextLine();
        } catch (IOException e) {
            return "";
        }
    }
}
