// Rust-specific features for metrics testing

fn with_question_operator(input: Option<i32>) -> Option<i32> {
    let value = input?;
    Some(value * 2)
}

fn multiple_question_operators(a: Option<i32>, b: Option<i32>) -> Option<i32> {
    let x = a?;
    let y = b?;
    Some(x + y)
}

fn question_in_expression(opt: Option<i32>) -> Option<i32> {
    Some(opt? * 2 + opt? * 3)
}

fn with_unwrap(opt: Option<i32>) -> i32 {
    opt.unwrap()
}

fn with_expect(opt: Option<i32>) -> i32 {
    opt.expect("value should exist")
}

fn multiple_unwraps(a: Option<i32>, b: Option<i32>) -> i32 {
    a.unwrap() + b.unwrap()
}

fn with_panic() {
    panic!("something went wrong");
}

fn conditional_panic(x: i32) {
    if x < 0 {
        panic!("negative value");
    }
}

fn multiple_panics(x: i32) {
    if x < 0 {
        panic!("negative");
    }
    if x == 0 {
        panic!("zero");
    }
}

fn mixed_error_handling(a: Option<i32>, b: Result<i32, String>) -> Result<i32, String> {
    let x = a.unwrap();
    let y = b?;
    if x < 0 {
        panic!("invalid");
    }
    Ok(x + y)
}

fn result_with_question(input: &str) -> Result<i32, std::num::ParseIntError> {
    let num = input.parse::<i32>()?;
    Ok(num * 2)
}

fn chained_question(a: Result<i32, String>, b: Result<i32, String>) -> Result<i32, String> {
    let x = a?;
    let y = b?;
    Ok(x + y)
}
