public class Simple
{
    public int SimpleMethod(int x)
    {
        return x + 1;
    }

    public int WithEarlyReturn(int x)
    {
        if (x < 0)
        {
            return 0;
        }
        return x;
    }
}
