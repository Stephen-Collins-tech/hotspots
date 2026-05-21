public class Exceptions
{
    public int TryCatch(int x)
    {
        try
        {
            return 10 / x;
        }
        catch (DivideByZeroException)
        {
            return -1;
        }
    }

    public int TryCatchFinally(int x)
    {
        int result = 0;
        try
        {
            result = 10 / x;
        }
        catch (Exception)
        {
            result = -1;
        }
        finally
        {
            Console.WriteLine("done");
        }
        return result;
    }

    public void ThrowOnNegative(int x)
    {
        if (x < 0)
        {
            throw new ArgumentException("x must be non-negative");
        }
    }
}
