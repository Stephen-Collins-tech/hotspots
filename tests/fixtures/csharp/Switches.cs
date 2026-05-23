public class Switches
{
    public string DayName(int day)
    {
        switch (day)
        {
            case 1:
                return "Monday";
            case 2:
                return "Tuesday";
            case 3:
                return "Wednesday";
            default:
                return "Other";
        }
    }

    public int Classify(int x)
    {
        switch (x)
        {
            case 0:
                return 0;
            case 1:
            case 2:
                return 1;
            default:
                return -1;
        }
    }

    public string NoDefault(int x)
    {
        switch (x)
        {
            case 1:
                return "one";
            case 2:
                return "two";
        }
        return "other";
    }

    public int WithBreak(int x)
    {
        int result = 0;
        switch (x)
        {
            case 1:
                result = 10;
                break;
            case 2:
                result = 20;
                break;
            default:
                result = -1;
                break;
        }
        return result;
    }
}
