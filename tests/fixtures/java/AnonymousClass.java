public class AnonymousClass {
    public void useAnonymousClass() {
        Runnable r = new Runnable() {
            @Override
            public void run() {
                if (someCondition()) {
                    doSomething();
                }
            }
        };
    }

    private boolean someCondition() {
        return true;
    }

    private void doSomething() {
        // do something
    }
}
