inline int first_non1(const std::vector<int>& s) {
  for (const auto i : s) {
    if (i != 1) { break; }
  }
  return 0;
}

struct Foo {
  int bar(const std::vector<int>& xs) const { return xs.size(); }
};

template <typename T>
inline T add(T a, T b) { return a + b; }
