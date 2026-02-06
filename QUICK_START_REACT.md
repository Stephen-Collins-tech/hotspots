# Quick Start: Analyzing React Projects

Hotspots now fully supports React projects in both TypeScript and JavaScript!

## Analyze React Components

```bash
# Single React component
hotspots analyze src/components/Button.tsx
hotspots analyze src/components/Modal.jsx

# Entire React project
hotspots analyze src/

# Get JSON output for CI
hotspots analyze src/ --format json
```

## What Gets Analyzed

**Main Component Functions:**
```tsx
function UserProfile({ user }) {
  // This function analyzed: UserProfile
  return <div>{user.name}</div>;
}
```

**Event Handlers:**
```tsx
function TodoList({ items }) {
  // This function analyzed separately: handleDelete
  const handleDelete = (id) => {
    if (confirm("Delete?")) {
      deleteItem(id);
    }
  };

  return (
    <ul>
      {items.map(item => (
        // This arrow function analyzed separately
        <li key={item.id} onClick={() => handleDelete(item.id)}>
          {item.text}
        </li>
      ))}
    </ul>
  );
}
// Result: 3 functions analyzed independently
```

## How JSX Affects Complexity

**✅ JSX Elements Don't Add Complexity:**
```tsx
function SimpleComponent() {
  return (
    <div>
      <h1>Title</h1>
      <p>Lots of nested JSX here...</p>
      <section>
        <article>
          <div>Even more JSX...</div>
        </article>
      </section>
    </div>
  );
}
// Metrics: CC=1, LRS=1.0 (simple function despite lots of JSX)
```

**✅ Control Flow in JSX DOES Add Complexity:**
```tsx
function ConditionalComponent({ isActive, hasData }) {
  return (
    <div>
      {isActive && <span>Active</span>}
      {hasData ? <Data/> : <NoData/>}
    </div>
  );
}
// Metrics: CC=2 (one for && operator)
// Note: && operator increases CC, but ternary in JSX may not
```

## Example Analysis Output

**Input: UserProfile.tsx**
```tsx
function UserProfile({ user, onUpdate }) {
  const handleStatusToggle = () => {
    if (!user) {
      console.error("No user");
      return;
    }
    if (user.role === 'admin') {
      const confirmed = confirm("Toggle admin?");
      if (!confirmed) return;
    }
    onUpdate({ ...user, isActive: !user.isActive });
  };

  return (
    <div>
      <h2>{user.name}</h2>
      <button onClick={handleStatusToggle}>Toggle</button>
    </div>
  );
}
```

**Output:**
```
LRS   Function           Metrics              Band
7.57  UserProfile        CC=4 ND=2 FO=5 NS=3  high
7.37  handleStatusToggle CC=8 ND=2 FO=3 NS=2  high
```

**Interpretation:**
- `UserProfile` component: Moderate complexity (LRS 7.57)
- `handleStatusToggle` event handler: High complexity (LRS 7.37)
- Consider refactoring the event handler (multiple nested conditions)

## Common Patterns

**1. Simple Component (Good!)**
```tsx
function Badge({ type, children }) {
  return <span className={`badge badge-${type}`}>{children}</span>;
}
// LRS: 1.0 ✅
```

**2. Component with Logic (Watch)**
```tsx
function UserCard({ user }) {
  const displayName = user.firstName + " " + user.lastName;
  const role = user.isAdmin ? "Admin" : "User";

  return (
    <div>
      <h3>{displayName}</h3>
      <span>{role}</span>
    </div>
  );
}
// LRS: ~2-3 ✅ (some logic, still reasonable)
```

**3. Complex Component (Refactor!)**
```tsx
function DataTable({ data, sortBy, filters }) {
  const filteredData = data.filter(row => {
    for (const [key, value] of Object.entries(filters)) {
      if (!value) continue;
      if (row[key] !== value) return false;
    }
    return true;
  });

  const sortedData = [...filteredData].sort((a, b) => {
    if (sortBy.direction === 'asc') {
      return a[sortBy.key] < b[sortBy.key] ? -1 : 1;
    } else {
      return a[sortBy.key] > b[sortBy.key] ? -1 : 1;
    }
  });

  return <table>{/* render sortedData */}</table>;
}
// LRS: 8-10 ⚠️ (consider extracting filtering/sorting logic)
```

## CI/CD Integration

**GitHub Actions:**
```yaml
- name: Analyze complexity
  run: |
    cargo install hotspots
    hotspots analyze src/ --format json > complexity.json

    # Check for critical complexity
    hotspots analyze src/ --min-lrs 9.0
```

**Pre-commit Hook:**
```bash
#!/bin/bash
# .git/hooks/pre-commit
hotspots analyze src/ --min-lrs 9.0 || {
  echo "❌ Critical complexity detected!"
  exit 1
}
```

## Tips

1. **Focus on High LRS Functions**: Prioritize refactoring functions with LRS > 9.0
2. **Event Handlers Often Complex**: Event handlers frequently have higher complexity - this is normal
3. **Extract Helper Functions**: Break complex logic into smaller, testable functions
4. **JSX Doesn't Count**: Don't worry about JSX structure - focus on logic complexity
5. **Anonymous Functions**: Use meaningful function names instead of inline arrows when complexity is high

## Next Steps

- Run `hotspots analyze src/ --top 10` to see your most complex functions
- Set up CI integration to track complexity over time
- Use `--format json` for programmatic analysis
- Check out `docs/language-support.md` for more details

## Supported File Types

All of these work out of the box:
- `.ts` `.tsx` (TypeScript)
- `.js` `.jsx` (JavaScript)
- `.mts` `.mtsx` `.cts` `.ctsx` (Module formats)
- `.mjs` `.mjsx` `.cjs` `.cjsx` (Module formats)

**12 file extensions total** - just point it at your `src/` directory!
