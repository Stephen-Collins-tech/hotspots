class MyClass:
    """A class with various method types."""

    def __init__(self, value):
        """Constructor method."""
        self.value = value

    def instance_method(self, x):
        """Simple instance method."""
        if x > 0:
            return self.value + x
        return self.value

    @classmethod
    def class_method(cls, x):
        """Class method with conditional."""
        if x < 0:
            return cls(0)
        return cls(x * 2)

    @staticmethod
    def static_method(x):
        """Static method with loop."""
        result = 0
        for i in range(x):
            if i % 2 == 0:
                result += i
        return result

    def method_with_exception_handling(self, data):
        """Method with try/except."""
        try:
            return self.process(data)
        except ValueError:
            return None
        except KeyError:
            return {}

    async def async_method(self, url):
        """Async instance method."""
        async with self.get_client() as client:
            if url.startswith("https"):
                return await client.fetch(url)
            return None
