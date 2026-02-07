// Simple Rust functions for basic metrics testing

fn empty_function() {
    // Empty function - minimal complexity
}

fn simple_calculation(x: i32, y: i32) -> i32 {
    let result = x + y;
    result
}

fn with_early_return(x: i32) -> i32 {
    if x < 0 {
        return 0;
    }
    x * 2
}

fn multiple_statements(a: i32, b: i32, c: i32) -> i32 {
    let x = a + b;
    let y = b + c;
    let z = x + y;
    println!("Result: {}", z);
    z
}

fn with_if_else(x: i32) -> &'static str {
    if x > 0 {
        "positive"
    } else {
        "non-positive"
    }
}

fn nested_if(x: i32) -> &'static str {
    if x > 0 {
        if x > 10 {
            "large"
        } else {
            "small"
        }
    } else {
        "negative"
    }
}

async fn async_function() -> String {
    "async".to_string()
}
