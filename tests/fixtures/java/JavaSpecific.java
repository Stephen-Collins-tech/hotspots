import java.util.List;
import java.util.stream.Collectors;

public class JavaSpecific {
    public void lambdaExpression(List<Integer> items) {
        items.forEach(item -> {
            if (item > 5) {
                System.out.println(item);
            }
        });
    }

    public List<Integer> streamOperations(List<Integer> items) {
        return items.stream()
            .filter(x -> x > 5)
            .map(x -> x * 2)
            .collect(Collectors.toList());
    }

    public String switchExpression(int value) {
        return switch (value) {
            case 0 -> "zero";
            case 1 -> "one";
            default -> "other";
        };
    }

    public synchronized void synchronizedMethod() {
        // critical section
    }
}
