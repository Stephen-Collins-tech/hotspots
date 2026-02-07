// Loop constructs in Rust

fn simple_loop(n: i32) -> i32 {
    let mut sum = 0;
    let mut i = 0;
    loop {
        if i >= n {
            break;
        }
        sum += i;
        i += 1;
    }
    sum
}

fn while_loop(n: i32) -> i32 {
    let mut sum = 0;
    let mut i = 0;
    while i < n {
        sum += i;
        i += 1;
    }
    sum
}

fn for_loop(items: &[i32]) -> i32 {
    let mut sum = 0;
    for item in items {
        sum += item;
    }
    sum
}

fn for_range(n: i32) -> i32 {
    let mut sum = 0;
    for i in 0..n {
        sum += i;
    }
    sum
}

fn loop_with_break(items: &[i32]) -> Option<i32> {
    for item in items {
        if *item > 10 {
            break;
        }
        if *item < 0 {
            return None;
        }
    }
    Some(0)
}

fn loop_with_continue(items: &[i32]) -> i32 {
    let mut sum = 0;
    for item in items {
        if *item < 0 {
            continue;
        }
        sum += item;
    }
    sum
}

fn nested_loops(matrix: &[&[i32]]) -> i32 {
    let mut sum = 0;
    for row in matrix {
        for &item in *row {
            if item > 0 {
                sum += item;
            }
        }
    }
    sum
}

fn while_with_break() -> i32 {
    let mut count = 0;
    while true {
        count += 1;
        if count >= 10 {
            break;
        }
    }
    count
}
