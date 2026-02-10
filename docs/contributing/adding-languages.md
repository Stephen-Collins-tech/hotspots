# Adding Language Support

Step-by-step guide to adding a new programming language to Hotspots.

## Overview

Adding language support involves:

1. **Parser Integration** - Integrate tree-sitter parser for the language
2. **CFG Builder** - Implement Control Flow Graph construction
3. **Test Fixtures** - Create test code files for the language
4. **Golden Files** - Generate expected output for determinism testing
5. **Documentation** - Update language support documentation

**Current Supported Languages:** TypeScript, JavaScript, Go, Java, Python, Rust

**Estimated Effort:** 7-14 days per language (varies by complexity)

---

## Prerequisites

Before starting:

- ✅ Familiarity with Rust programming
- ✅ Understanding of Control Flow Graphs (CFGs)
- ✅ Knowledge of the target language's syntax and semantics
- ✅ tree-sitter parser exists for the target language
- ✅ Read [Architecture Overview](../architecture/overview.md)
- ✅ Review existing language implementations (Go, Python, Rust)

---

## Step-by-Step Guide

### Step 1: Parser Integration

#### 1.1 Add tree-sitter Dependency

Edit `hotspots-core/Cargo.toml`:

```toml
[dependencies]
# ... existing dependencies ...
tree-sitter-<language> = "0.x.y"
```

**Find tree-sitter parsers:** Check [tree-sitter GitHub](https://github.com/tree-sitter) or [crates.io](https://crates.io).

#### 1.2 Create Language Module

Create `hotspots-core/src/language/<language>/mod.rs`:

```rust
//! <Language> language support
//!
//! This module provides <Language> parsing, function discovery, and CFG building
//! using the tree-sitter-<language> parser.

pub mod cfg_builder;
pub mod parser;

pub use cfg_builder::<Language>CfgBuilder;
pub use parser::<Language>Parser;
```

#### 1.3 Implement Parser

Create `hotspots-core/src/language/<language>/parser.rs`:

```rust
use tree_sitter::{Node, Parser, TreeCursor};
use anyhow::{Context, Result};
use crate::language::function_body::FunctionBody;

pub struct <Language>Parser {
    parser: Parser,
}

impl <Language>Parser {
    pub fn new() -> Result<Self> {
        let mut parser = Parser::new();
        let language = tree_sitter_<language>::LANGUAGE.into();
        parser.set_language(&language)
            .context("Failed to load <Language> grammar")?;

        Ok(Self { parser })
    }

    /// Parse source code and discover functions
    pub fn discover_functions(&mut self, source: &str) -> Result<Vec<FunctionBody>> {
        let tree = self.parser.parse(source, None)
            .context("Failed to parse <Language> source")?;

        let root = tree.root_node();
        let mut functions = Vec::new();

        // Walk AST to find function declarations
        let mut cursor = root.walk();
        self.visit_node(&mut cursor, source, &mut functions);

        // Sort by source position for determinism
        functions.sort_by_key(|f| match f {
            FunctionBody::<Language> { body_node, .. } => *body_node,
            _ => unreachable!(),
        });

        Ok(functions)
    }

    fn visit_node(&self, cursor: &mut TreeCursor, source: &str, functions: &mut Vec<FunctionBody>) {
        let node = cursor.node();

        // Check if this node is a function declaration
        match node.kind() {
            "function_declaration" | "method_declaration" => {
                if let Some(func) = self.extract_function(node, source) {
                    functions.push(func);
                }
            }
            _ => {}
        }

        // Recursively visit children
        if cursor.goto_first_child() {
            loop {
                self.visit_node(cursor, source, functions);
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
            cursor.goto_parent();
        }
    }

    fn extract_function(&self, node: Node, source: &str) -> Option<FunctionBody> {
        // Extract function name
        let name_node = node.child_by_field_name("name")?;
        let function_name = &source[name_node.byte_range()];

        // Extract function body
        let body_node = node.child_by_field_name("body")?;
        let body_source = &source[body_node.byte_range()];

        Some(FunctionBody::<Language> {
            body_node: body_node.id(),
            source: body_source.to_string(),
        })
    }
}

impl crate::language::LanguageParser for <Language>Parser {
    fn discover_functions(&mut self, source: &str) -> Result<Vec<FunctionBody>> {
        self.discover_functions(source)
    }
}
```

**Key Considerations:**
- **Node kinds:** Check tree-sitter grammar for exact node type names
- **Field names:** Use `child_by_field_name()` for reliable access
- **Determinism:** Always sort functions by source position
- **Multiple function types:** Handle methods, closures, lambda, etc.

#### 1.4 Add to Language Enum

Edit `hotspots-core/src/language/mod.rs`:

```rust
pub mod <language>;

pub enum Language {
    // ... existing languages ...
    <Language>,
}

impl Language {
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext {
            // ... existing extensions ...
            "<ext>" => Some(Language::<Language>),
            _ => None,
        }
    }

    pub fn extensions(&self) -> &[&str] {
        match self {
            // ... existing languages ...
            Language::<Language> => &["<ext>"],
        }
    }

    pub fn name(&self) -> &str {
        match self {
            // ... existing languages ...
            Language::<Language> => "<Language>",
        }
    }
}

pub use <language>::{<Language>Parser, <Language>CfgBuilder};
```

#### 1.5 Add FunctionBody Variant

Edit `hotspots-core/src/language/function_body.rs`:

```rust
pub enum FunctionBody {
    // ... existing variants ...
    <Language> {
        body_node: usize,
        source: String,
    },
}

impl FunctionBody {
    pub fn is_<language>(&self) -> bool {
        matches!(self, FunctionBody::<Language> { .. })
    }

    pub fn as_<language>(&self) -> Option<(&usize, &str)> {
        if let FunctionBody::<Language> { body_node, source } = self {
            Some((body_node, source))
        } else {
            None
        }
    }
}
```

---

### Step 2: CFG Builder

#### 2.1 Create CFG Builder

Create `hotspots-core/src/language/<language>/cfg_builder.rs`:

```rust
use crate::cfg::{Cfg, CfgNode};
use anyhow::Result;

pub struct <Language>CfgBuilder;

impl <Language>CfgBuilder {
    /// Build CFG from function body
    pub fn build_cfg(source: &str) -> Result<Cfg> {
        // Re-parse source to get tree-sitter tree
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_<language>::LANGUAGE.into())?;
        let tree = parser.parse(source, None)
            .ok_or_else(|| anyhow::anyhow!("Failed to parse function body"))?;

        let root = tree.root_node();

        // Initialize CFG
        let mut cfg = Cfg::new();
        let entry = cfg.add_node(CfgNode::Entry);
        let exit = cfg.add_node(CfgNode::Exit);

        // Build CFG from AST
        let last = Self::visit_block(&mut cfg, root, entry, exit)?;
        cfg.add_edge(last, exit);

        Ok(cfg)
    }

    fn visit_block(cfg: &mut Cfg, node: tree_sitter::Node, entry: usize, exit: usize) -> Result<usize> {
        let mut current = entry;

        // Iterate through statements in block
        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                current = Self::visit_statement(cfg, cursor.node(), current, exit)?;
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }

        Ok(current)
    }

    fn visit_statement(cfg: &mut Cfg, node: tree_sitter::Node, entry: usize, exit: usize) -> Result<usize> {
        match node.kind() {
            "if_statement" => Self::visit_if(cfg, node, entry, exit),
            "while_statement" => Self::visit_while(cfg, node, entry, exit),
            "for_statement" => Self::visit_for(cfg, node, entry, exit),
            "return_statement" => {
                // Early return - route directly to exit
                cfg.add_edge(entry, exit);
                Ok(entry)  // Dead code after return
            }
            _ => {
                // Simple statement - single node
                let stmt_node = cfg.add_node(CfgNode::Statement);
                cfg.add_edge(entry, stmt_node);
                Ok(stmt_node)
            }
        }
    }

    fn visit_if(cfg: &mut Cfg, node: tree_sitter::Node, entry: usize, exit: usize) -> Result<usize> {
        // Create condition node
        let cond = cfg.add_node(CfgNode::Condition);
        cfg.add_edge(entry, cond);

        // Then branch
        let then_node = node.child_by_field_name("consequence").unwrap();
        let then_last = Self::visit_block(cfg, then_node, cond, exit)?;

        // Else branch (optional)
        let join = cfg.add_node(CfgNode::Join);
        if let Some(else_node) = node.child_by_field_name("alternative") {
            let else_last = Self::visit_block(cfg, else_node, cond, exit)?;
            cfg.add_edge(else_last, join);
        } else {
            cfg.add_edge(cond, join);  // No else - fall through
        }

        cfg.add_edge(then_last, join);
        Ok(join)
    }

    fn visit_while(cfg: &mut Cfg, node: tree_sitter::Node, entry: usize, exit: usize) -> Result<usize> {
        // Loop header (condition)
        let header = cfg.add_node(CfgNode::LoopHeader);
        cfg.add_edge(entry, header);

        // Loop body
        let body_node = node.child_by_field_name("body").unwrap();
        let body_last = Self::visit_block(cfg, body_node, header, exit)?;

        // Back edge to loop header
        cfg.add_edge(body_last, header);

        // Loop exit
        let join = cfg.add_node(CfgNode::Join);
        cfg.add_edge(header, join);  // Exit when condition false

        Ok(join)
    }

    // ... implement visit_for, visit_switch, etc. ...
}

impl crate::language::cfg_builder::CfgBuilder for <Language>CfgBuilder {
    fn build_cfg(body: &crate::language::function_body::FunctionBody) -> Result<Cfg> {
        if let Some((_, source)) = body.as_<language>() {
            Self::build_cfg(source)
        } else {
            anyhow::bail!("Expected <Language> function body")
        }
    }
}
```

**Key Considerations:**
- **Control flow constructs:** Handle if, while, for, switch, try/catch
- **Early exits:** return, throw, break, continue route to appropriate nodes
- **Loop context:** Track loop headers for break/continue routing
- **Nesting depth:** Track depth during traversal for ND metric
- **Language-specific:** Handle language-specific constructs (e.g., Python else-on-loops, Go defer)

#### 2.2 Register CFG Builder

Edit `hotspots-core/src/language/cfg_builder.rs`:

```rust
pub fn create_cfg_builder(body: &FunctionBody) -> Box<dyn CfgBuilder> {
    match body {
        // ... existing languages ...
        FunctionBody::<Language> { .. } => Box::new(<language>::<Language>CfgBuilder),
    }
}
```

#### 2.3 Update Analysis Dispatcher

Edit `hotspots-core/src/analysis.rs`:

```rust
pub fn create_parser(lang: Language) -> Result<Box<dyn LanguageParser>> {
    match lang {
        // ... existing languages ...
        Language::<Language> => {
            Ok(Box::new(<language>::<Language>Parser::new()
                .context("Failed to create <Language> parser")?))
        }
    }
}
```

---

### Step 3: Testing

#### 3.1 Create Test Fixtures

Create `tests/fixtures/<language>/` directory with test files:

**simple.ext** - Basic functions:
```<language>
// Simple function (low complexity)
function simpleFunction(x) {
    return x + 1;
}

// Function with early return
function withEarlyReturn(x) {
    if (x < 0) {
        return 0;
    }
    return x;
}
```

**loops.ext** - Loop constructs:
```<language>
// While loop
function whileLoop(n) {
    int i = 0;
    while (i < n) {
        i++;
    }
    return i;
}

// For loop with break
function forLoopWithBreak(items[]) {
    for (item in items) {
        if (item > 10) {
            break;
        }
    }
    return items[0];
}

// Nested loops
function nestedLoops(matrix[][]) {
    for (row in matrix) {
        for (col in row) {
            if (col == 0) {
                continue;
            }
        }
    }
}
```

**branching.ext** - Conditional logic:
```<language>
// If/else chains
function ifElseChain(value) {
    if (value < 0) {
        return "negative";
    } else if (value == 0) {
        return "zero";
    } else {
        return "positive";
    }
}

// Switch statement
function switchStatement(value) {
    switch (value) {
        case 0:
            return "zero";
        case 1:
            return "one";
        default:
            return "other";
    }
}
```

**Create 5-7 test files covering:**
- Simple functions (low complexity)
- Loops (while, for, do-while)
- Conditionals (if, else, switch)
- Early exits (return, throw)
- Nesting (nested if, nested loops)
- Language-specific constructs

#### 3.2 Generate Golden Files

```bash
# Build release binary
cargo build --release

# Generate golden output for each fixture
./target/release/hotspots analyze tests/fixtures/<language>/simple.ext --format json > tests/golden/<language>-simple.json
./target/release/hotspots analyze tests/fixtures/<language>/loops.ext --format json > tests/golden/<language>-loops.json
./target/release/hotspots analyze tests/fixtures/<language>/branching.ext --format json > tests/golden/<language>-branching.json

# Verify output manually
cat tests/golden/<language>-simple.json | jq .
```

**Golden file checklist:**
- ✅ Includes all expected functions
- ✅ Metrics (CC, ND, FO, NS) look correct
- ✅ LRS calculations are reasonable
- ✅ Risk bands are appropriate
- ✅ No parsing errors

#### 3.3 Add Unit Tests

Create `hotspots-core/tests/<language>_tests.rs`:

```rust
#[cfg(test)]
mod <language>_tests {
    use hotspots_core::language::<language>::<Language>Parser;
    use hotspots_core::analyze_function;

    #[test]
    fn test_simple_function() {
        let source = r#"
            function simple(x) {
                return x + 1;
            }
        "#;

        let mut parser = <Language>Parser::new().unwrap();
        let functions = parser.discover_functions(source).unwrap();

        assert_eq!(functions.len(), 1);

        let report = analyze_function(&functions[0], "test.ext").unwrap();
        assert_eq!(report.metrics.cc, 1);  // No branches
        assert_eq!(report.metrics.nd, 0);  // No nesting
    }

    #[test]
    fn test_if_statement() {
        let source = r#"
            function withIf(x) {
                if (x > 0) {
                    return x;
                }
                return 0;
            }
        "#;

        let mut parser = <Language>Parser::new().unwrap();
        let functions = parser.discover_functions(source).unwrap();
        let report = analyze_function(&functions[0], "test.ext").unwrap();

        assert_eq!(report.metrics.cc, 2);  // if adds +1
        assert_eq!(report.metrics.nd, 1);  // one level deep
        assert_eq!(report.metrics.ns, 1);  // early return
    }

    // Add more tests...
}
```

#### 3.4 Run Tests

```bash
# Run all tests
cargo test

# Run language-specific tests
cargo test <language>

# Run with output
cargo test <language> -- --nocapture

# Verify golden tests
cargo test --test golden_tests
```

---

### Step 4: Documentation

#### 4.1 Update Language Support Docs

Edit `docs/reference/language-support.md`:

```markdown
## <Language>

**Supported:** Yes (v1.x.x+)
**File Extensions:** `.<ext>`
**tree-sitter Parser:** `tree-sitter-<language>`

### Function Detection

- Function declarations
- Method declarations
- Class methods (if applicable)
- Lambda expressions / closures

### Complexity Metrics

**Cyclomatic Complexity (CC):**
- `if`, `else if` statements (+1 each)
- `while`, `for`, `do-while` loops (+1 each)
- `switch` cases (+1 per case)
- Ternary operators `? :` (+1)
- Logical operators `&&`, `||` (+1 each)
- Language-specific constructs...

**Nesting Depth (ND):**
- Maximum depth of nested control structures

**Fan-Out (FO):**
- Function/method calls
- Constructor calls

**Non-Structured Exits (NS):**
- `return` statements
- `throw` statements
- `break` statements
- `continue` statements

### Language-Specific Behavior

**[Document unique behaviors]**

Example: Python's else-on-loops, Go's defer statements, etc.

### Example Analysis

```<language>
function complexFunction(data) {
    if (data.type === 'A') {
        for (item in data.items) {
            if (item.active) {
                return processItem(item);
            }
        }
    }
    return null;
}
```

**Metrics:**
- CC: 4 (if type, for loop, if active, implicit +1)
- ND: 2 (if nested in for)
- FO: 1 (processItem call)
- NS: 1 (early return)
- LRS: ~4.8 (moderate risk)
```

#### 4.2 Update README.md

Update supported languages list and examples.

#### 4.3 Add Examples

Create `examples/<language>/` with sample projects.

---

## Testing Checklist

Before submitting PR:

- [ ] Parser correctly discovers all function types
- [ ] CFG builder handles all control flow constructs
- [ ] Metrics match manual calculation
- [ ] All unit tests pass
- [ ] Golden tests pass (deterministic output)
- [ ] No compilation warnings
- [ ] Code follows Rust conventions (`cargo fmt`, `cargo clippy`)
- [ ] Documentation updated
- [ ] Examples added

---

## Common Pitfalls

### 1. Non-Deterministic Output

**Problem:** Functions appear in random order.

**Solution:** Always sort by source position:
```rust
functions.sort_by_key(|f| f.start_position());
```

### 2. Missing Function Types

**Problem:** Only detecting some functions (e.g., missing methods, closures).

**Solution:** Check all function node kinds in tree-sitter grammar:
```bash
# Inspect grammar
tree-sitter parse examples/code.ext --debug
```

### 3. Incorrect CC Calculation

**Problem:** CC doesn't match manual count.

**Solution:** Debug CFG visualization, check:
- All decision points counted (+1 for each)
- No double-counting
- Implicit +1 for function entry

### 4. Break/Continue Routing

**Problem:** Break/continue go to wrong CFG nodes.

**Solution:** Track loop context stack:
```rust
struct LoopContext {
    header: usize,
    exit: usize,
}

let mut loop_stack: Vec<LoopContext> = Vec::new();
```

### 5. Parser Crashes

**Problem:** Parser panics on certain code.

**Solution:** Add error handling:
```rust
let body_node = node.child_by_field_name("body")
    .ok_or_else(|| anyhow::anyhow!("Missing function body"))?;
```

---

## Reference Implementations

**Good examples to study:**
1. **Go** (`hotspots-core/src/language/go/`) - Simple, clean implementation
2. **Python** (`hotspots-core/src/language/python/`) - Complex language features
3. **Rust** (`hotspots-core/src/language/rust/`) - Pattern matching, complex control flow

---

## Getting Help

- 💬 [GitHub Discussions](https://github.com/Stephen-Collins-tech/hotspots/discussions)
- 📧 [Open an Issue](https://github.com/Stephen-Collins-tech/hotspots/issues)
- 📖 [Architecture Docs](../architecture/overview.md)

---

## After Implementation

Once language support is complete:

1. **Open PR** with all changes
2. **Update CHANGELOG.md**
3. **Announce** in discussions
4. **Add to roadmap** for future enhancements

**Congratulations!** You've added language support to Hotspots. 🎉
