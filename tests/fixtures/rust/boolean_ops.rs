// Boolean operators in Rust

fn simple_and(x: i32, y: i32) -> bool {
    x > 0 && y > 0
}

fn simple_or(x: i32, y: i32) -> bool {
    x > 0 || y > 0
}

fn multiple_and(a: i32, b: i32, c: i32) -> bool {
    a > 0 && b > 0 && c > 0
}

fn multiple_or(a: i32, b: i32, c: i32) -> bool {
    a > 0 || b > 0 || c > 0
}

fn mixed_operators(a: i32, b: i32, c: i32) -> bool {
    (a > 0 && b > 0) || c > 0
}

fn complex_condition(x: i32, y: i32, z: i32) -> bool {
    (x > 0 || x < -10) && (y > 0 || y < -10) && z == 0
}

fn nested_conditions(a: i32, b: i32, c: i32, d: i32) -> &'static str {
    if (a > 0 && b > 0) || (c > 0 && d > 0) {
        "positive"
    } else {
        "negative"
    }
}

fn short_circuit_and(opt: Option<i32>) -> bool {
    opt.is_some() && opt.unwrap() > 0
}

fn short_circuit_or(opt: Option<i32>) -> bool {
    opt.is_none() || opt.unwrap() > 0
}

fn while_with_boolean(x: &mut i32, y: &mut i32) -> i32 {
    let mut count = 0;
    while *x > 0 && *y > 0 {
        *x -= 1;
        *y -= 1;
        count += 1;
    }
    count
}

fn for_with_filter(items: &[i32]) -> i32 {
    let mut sum = 0;
    for item in items {
        if *item > 0 && *item < 100 {
            sum += item;
        }
    }
    sum
}
