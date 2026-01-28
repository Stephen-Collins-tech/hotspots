// Component with conditional rendering using ternary and &&
function ConditionalComponent(props: { isLoggedIn: boolean; hasData: boolean }) {
  return (
    <div>
      {props.isLoggedIn ? (
        <h1>Welcome</h1>
      ) : (
        <h1>Please log in</h1>
      )}
      {props.hasData && <p>Data available</p>}
    </div>
  );
}
