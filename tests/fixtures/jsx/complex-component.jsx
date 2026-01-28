// Complex component with loops, conditionals, and event handlers
function ComplexComponent(props) {
  const handleClick = (id) => {
    if (id < 0) {
      return;
    }

    for (const item of props.items) {
      if (item.id === id) {
        if (item.active) {
          console.log("Already active");
          break;
        }
      }
    }
  };

  return (
    <div>
      {props.items.map((item) => (
        <div key={item.id} onClick={() => handleClick(item.id)}>
          {item.active ? (
            <span>Active: {item.id}</span>
          ) : (
            <span>Inactive: {item.id}</span>
          )}
        </div>
      ))}
    </div>
  );
}
