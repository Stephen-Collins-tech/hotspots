# Generated fixtures

`go_large.go` is pre-generated, not repeated with the `repeated!` macro the
other languages use. Go's `package`/`import` block can't be duplicated
inside one file the way the other languages' fixtures (which carry no
package/using/import lines) can — so this file strips `go_specific.go`'s
header once and repeats only its function bodies 16 times.

Regenerate after `tests/fixtures/go/go_specific.go` changes:

```bash
python3 - <<'PY'
src = open("../../../tests/fixtures/go/go_specific.go").read()
body_start = src.index(")\n", src.index("import")) + 2
header = "package fixtures\n\nimport (\n\t\"fmt\"\n\t\"time\"\n)\n\n"
body = src[body_start:].lstrip("\n")
open("go_large.go", "w").write(header + body * 16)
PY
```
