import React, { useState } from 'react';

// React component in a plain .js file (React webpack convention)
function Counter({ initialCount, onCountChange }) {
  const [count, setCount] = useState(initialCount || 0);

  const increment = () => {
    const next = count + 1;
    setCount(next);
    if (onCountChange) {
      onCountChange(next);
    }
  };

  const decrement = () => {
    if (count <= 0) return;
    const next = count - 1;
    setCount(next);
    if (onCountChange) {
      onCountChange(next);
    }
  };

  return (
    <div className="counter">
      <button onClick={decrement}>-</button>
      <span>{count}</span>
      <button onClick={increment}>+</button>
    </div>
  );
}

export default Counter;
