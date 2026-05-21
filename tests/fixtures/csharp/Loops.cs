public class Loops
{
    public int ForLoop(int n)
    {
        int sum = 0;
        for (int i = 0; i < n; i++)
        {
            sum += i;
        }
        return sum;
    }

    public int WhileLoop(int n)
    {
        int i = 0;
        while (i < n)
        {
            i++;
        }
        return i;
    }

    public int DoWhileLoop(int n)
    {
        int i = 0;
        do
        {
            i++;
        } while (i < n);
        return i;
    }

    public int ForeachLoop(int[] items)
    {
        int sum = 0;
        foreach (var item in items)
        {
            sum += item;
        }
        return sum;
    }
}
